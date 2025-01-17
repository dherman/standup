use std::fmt::{self, Display};
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::Tool;
use crate::error::{Context, ErrorKind, Fallible};
use crate::fs::{remove_dir_if_exists, rename, symlink_dir};
use crate::layout::volta_home;
use crate::platform::{Image, PlatformSpec};
use crate::session::Session;
use crate::style::{success_prefix, tool_version};
use crate::sync::VoltaLock;
use crate::version::VersionSpec;
use fs_utils::ensure_containing_dir_exists;
use log::info;
use tempfile::{tempdir_in, TempDir};

mod configure;
mod install;
mod manager;
mod metadata;
mod uninstall;

pub use manager::PackageManager;
pub use metadata::{BinConfig, PackageConfig, PackageManifest};
pub use uninstall::uninstall;

/// The Tool implementation for installing 3rd-party global packages
pub struct Package {
    name: String,
    version: VersionSpec,
    staging: TempDir,
}

impl Package {
    pub fn new(name: String, version: VersionSpec) -> Fallible<Self> {
        let staging = setup_staging_directory(PackageManager::Npm, NeedsScope::No)?;

        Ok(Package {
            name,
            version,
            staging,
        })
    }

    pub fn run_install(&self, platform_image: &Image) -> Fallible<()> {
        install::run_global_install(
            self.to_string(),
            self.staging.path().to_owned(),
            platform_image,
        )
    }

    pub fn complete_install(self, image: &Image) -> Fallible<PackageManifest> {
        let manager = PackageManager::Npm;
        let manifest =
            configure::parse_manifest(&self.name, self.staging.path().to_owned(), manager)?;

        persist_install(&self.name, &self.version, self.staging.path())?;
        link_package_to_shared_dir(&self.name, manager)?;
        configure::write_config_and_shims(&self.name, &manifest, image, manager)?;

        Ok(manifest)
    }
}

impl Tool for Package {
    fn fetch(self: Box<Self>, _session: &mut Session) -> Fallible<()> {
        Err(ErrorKind::CannotFetchPackage {
            package: self.to_string(),
        }
        .into())
    }

    fn install(self: Box<Self>, session: &mut Session) -> Fallible<()> {
        let _lock = VoltaLock::acquire();

        let default_image = session
            .default_platform()?
            .map(PlatformSpec::as_default)
            .ok_or(ErrorKind::NoPlatform)?
            .checkout(session)?;

        self.run_install(&default_image)?;
        let manifest = self.complete_install(&default_image)?;

        let bins = manifest.bin.join(", ");

        if bins.is_empty() {
            info!(
                "{} installed {}",
                success_prefix(),
                tool_version(manifest.name, manifest.version)
            );
        } else {
            info!(
                "{} installed {} with executables: {}",
                success_prefix(),
                tool_version(manifest.name, manifest.version),
                bins
            );
        }

        Ok(())
    }

    fn pin(self: Box<Self>, _session: &mut Session) -> Fallible<()> {
        Err(ErrorKind::CannotPinPackage { package: self.name }.into())
    }

    fn uninstall(self: Box<Self>, _session: &mut Session) -> Fallible<()> {
        // For packages, specifically report that we do not support uninstalling
        // specific versions. For package managers, we currently
        // *intentionally* let this fall through to inform the user that we do
        // not support uninstalling those *at all*.
        let VersionSpec::None = &self.version else {
            return Err(ErrorKind::Unimplemented {
                feature: "uninstalling specific versions of tools".into(),
            }
            .into());
        };
        uninstall(&self.name)
    }
}

impl Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.version {
            VersionSpec::None => f.write_str(&self.name),
            _ => f.write_str(&tool_version(&self.name, &self.version)),
        }
    }
}

/// Helper struct for direct installs through `npm i -g` or `yarn global add`
///
/// Provides methods to simplify installing into a staging directory and then moving that install
/// into the proper location after it is complete.
///
/// Note: We don't always know the name of the package up-front, as the install could be from a
/// tarball or a git coordinate. If we do know ahead of time, then we can skip looking it up
pub struct DirectInstall {
    staging: TempDir,
    manager: PackageManager,
    name: Option<String>,
}

impl DirectInstall {
    pub fn new(manager: PackageManager) -> Fallible<Self> {
        let staging = setup_staging_directory(manager, NeedsScope::No)?;

        Ok(DirectInstall {
            staging,
            manager,
            name: None,
        })
    }

    pub fn with_name(manager: PackageManager, name: String) -> Fallible<Self> {
        let staging = setup_staging_directory(manager, name.contains('/').into())?;

        Ok(DirectInstall {
            staging,
            manager,
            name: Some(name),
        })
    }

    pub fn setup_command(&self, command: &mut Command) {
        self.manager
            .setup_global_command(command, self.staging.path().to_owned());
    }

    pub fn complete_install(self, image: &Image) -> Fallible<()> {
        let DirectInstall {
            staging,
            name,
            manager,
        } = self;

        let name = name
            .or_else(|| manager.get_installed_package(staging.path().to_owned()))
            .ok_or(ErrorKind::InstalledPackageNameError)?;
        let manifest = configure::parse_manifest(&name, staging.path().to_owned(), manager)?;

        persist_install(&name, &manifest.version, staging.path())?;
        link_package_to_shared_dir(&name, manager)?;
        configure::write_config_and_shims(&name, &manifest, image, manager)
    }
}

/// Helper struct for direct in-place upgrades using `npm update -g` or `yarn global upgrade`
///
/// Upgrades the requested package directly in the image directory
pub struct InPlaceUpgrade {
    package: String,
    directory: PathBuf,
    manager: PackageManager,
}

impl InPlaceUpgrade {
    pub fn new(package: String, manager: PackageManager) -> Fallible<Self> {
        let directory = volta_home()?.package_image_dir(&package);

        Ok(Self {
            package,
            directory,
            manager,
        })
    }

    /// Check for possible failure cases with the package to be upgraded
    ///     - The package is not installed as a global
    ///     - The package exists, but was installed with a different package manager
    pub fn check_upgraded_package(&self) -> Fallible<()> {
        let config =
            PackageConfig::from_file(volta_home()?.default_package_config_file(&self.package))
                .with_context(|| ErrorKind::UpgradePackageNotFound {
                    package: self.package.clone(),
                    manager: self.manager,
                })?;

        if config.manager != self.manager {
            Err(ErrorKind::UpgradePackageWrongManager {
                package: self.package.clone(),
                manager: config.manager,
            }
            .into())
        } else {
            Ok(())
        }
    }

    pub fn setup_command(&self, command: &mut Command) {
        self.manager
            .setup_global_command(command, self.directory.clone());
    }

    pub fn complete_upgrade(self, image: &Image) -> Fallible<()> {
        let manifest = configure::parse_manifest(&self.package, self.directory, self.manager)?;

        link_package_to_shared_dir(&self.package, self.manager)?;
        configure::write_config_and_shims(&self.package, &manifest, image, self.manager)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum NeedsScope {
    Yes,
    No,
}

impl From<bool> for NeedsScope {
    fn from(value: bool) -> Self {
        if value {
            NeedsScope::Yes
        } else {
            NeedsScope::No
        }
    }
}

/// Create the temporary staging directory we will use to install and ensure expected
/// subdirectories exist within it
fn setup_staging_directory(manager: PackageManager, needs_scope: NeedsScope) -> Fallible<TempDir> {
    // Workaround to ensure relative symlinks continue to work.
    // The final installed location of packages is:
    //      $VOLTA_HOME/tools/image/packages/{name}/
    // To ensure that the temp directory has the same amount of nesting, we use:
    //      $VOLTA_HOME/tmp/image/packages/{tempdir}/
    // This way any relative symlinks will have the same amount of nesting and will remain valid
    // even when the directory is persisted.
    // We also need to handle the case when the linked package has a scope, which requires another
    // level of nesting
    let mut staging_root = volta_home()?.tmp_dir().to_owned();
    staging_root.push("image");
    staging_root.push("packages");
    if needs_scope == NeedsScope::Yes {
        staging_root.push("scope");
    }
    create_dir_all(&staging_root).with_context(|| ErrorKind::ContainingDirError {
        path: staging_root.clone(),
    })?;
    let staging = tempdir_in(&staging_root).with_context(|| ErrorKind::CreateTempDirError {
        in_dir: staging_root,
    })?;

    let source_dir = manager.source_dir(staging.path().to_owned());
    ensure_containing_dir_exists(&source_dir)
        .with_context(|| ErrorKind::ContainingDirError { path: source_dir })?;

    let binary_dir = manager.binary_dir(staging.path().to_owned());
    ensure_containing_dir_exists(&binary_dir)
        .with_context(|| ErrorKind::ContainingDirError { path: binary_dir })?;

    Ok(staging)
}

fn persist_install<V>(package_name: &str, package_version: V, staging_dir: &Path) -> Fallible<()>
where
    V: Display,
{
    let package_dir = volta_home()?.package_image_dir(package_name);

    remove_dir_if_exists(&package_dir)?;

    // Handle scoped packages (@vue/cli), which have an extra directory for the scope
    ensure_containing_dir_exists(&package_dir).with_context(|| ErrorKind::ContainingDirError {
        path: package_dir.to_owned(),
    })?;

    rename(staging_dir, &package_dir).with_context(|| ErrorKind::SetupToolImageError {
        tool: package_name.into(),
        version: package_version.to_string(),
        dir: package_dir,
    })?;

    Ok(())
}

fn link_package_to_shared_dir(package_name: &str, manager: PackageManager) -> Fallible<()> {
    let home = volta_home()?;
    let mut source = manager.source_dir(home.package_image_dir(package_name));
    source.push(package_name);

    let target = home.shared_lib_dir(package_name);

    remove_dir_if_exists(&target)?;

    // Handle scoped packages (@vue/cli), which have an extra directory for the scope
    ensure_containing_dir_exists(&target).with_context(|| ErrorKind::ContainingDirError {
        path: target.clone(),
    })?;

    symlink_dir(source, target).with_context(|| ErrorKind::CreateSharedLinkError {
        name: package_name.into(),
    })
}
