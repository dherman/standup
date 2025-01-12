//! Provides the `Project` type, which represents a Node project tree in
//! the filesystem.

use std::env;
use std::ffi::OsStr;
use std::iter::once;
use std::path::{Path, PathBuf};

use node_semver::Version;
use once_cell::unsync::OnceCell;

use crate::error::{Context, ErrorKind, Fallible, VoltaError};
use crate::layout::volta_home;
use crate::platform::PlatformSpec;
use crate::tool::BinConfig;
use chain_map::ChainMap;
use indexmap::IndexSet;

mod serial;
#[cfg(test)]
mod tests;

use serial::{update_manifest, Manifest, ManifestKey};

/// A lazily loaded Project
pub struct LazyProject {
    project: OnceCell<Option<Project>>,
}

impl LazyProject {
    pub fn init() -> Self {
        LazyProject {
            project: OnceCell::new(),
        }
    }

    pub fn get(&self) -> Fallible<Option<&Project>> {
        let project = self.project.get_or_try_init(Project::for_current_dir)?;
        Ok(project.as_ref())
    }

    pub fn get_mut(&mut self) -> Fallible<Option<&mut Project>> {
        let _ = self.project.get_or_try_init(Project::for_current_dir)?;
        Ok(self.project.get_mut().unwrap().as_mut())
    }
}

/// A Node project workspace in the filesystem
#[cfg_attr(test, derive(Debug))]
pub struct Project {
    manifest_file: PathBuf,
    workspace_manifests: IndexSet<PathBuf>,
    dependencies: ChainMap<String, String>,
    platform: Option<PlatformSpec>,
}

impl Project {
    /// Creates an optional Project instance from the current directory
    fn for_current_dir() -> Fallible<Option<Self>> {
        let current_dir = env::current_dir().with_context(|| ErrorKind::CurrentDirError)?;
        Self::for_dir(current_dir)
    }

    /// Creates an optional Project instance from the specified directory
    ///
    /// Will search ancestors to find a `package.json` and use that as the root of the project
    fn for_dir(base_dir: PathBuf) -> Fallible<Option<Self>> {
        match find_closest_root(base_dir) {
            Some(mut project) => {
                project.push("package.json");
                Self::from_file(project).map(Some)
            }
            None => Ok(None),
        }
    }

    /// Creates a Project instance from the given package manifest file (`package.json`)
    fn from_file(manifest_file: PathBuf) -> Fallible<Self> {
        let manifest = Manifest::from_file(&manifest_file)?;
        let mut dependencies: ChainMap<String, String> = manifest.dependency_maps.collect();
        let mut workspace_manifests = IndexSet::new();
        let mut platform = manifest.platform;
        let mut extends = manifest.extends;

        // Iterate the `volta.extends` chain, parsing each file in turn
        while let Some(path) = extends {
            // Detect cycles to prevent infinite looping
            if path == manifest_file || workspace_manifests.contains(&path) {
                let mut paths = vec![manifest_file];
                paths.extend(workspace_manifests);

                return Err(ErrorKind::ExtensionCycleError {
                    paths,
                    duplicate: path,
                }
                .into());
            }

            let manifest = Manifest::from_file(&path)?;
            workspace_manifests.insert(path);
            dependencies.extend(manifest.dependency_maps);

            platform = match (platform, manifest.platform) {
                (Some(base), Some(ext)) => Some(base.merge(ext)),
                (Some(plat), None) | (None, Some(plat)) => Some(plat),
                (None, None) => None,
            };

            extends = manifest.extends;
        }

        let platform = match platform.map(TryInto::try_into).transpose()? {
            Some(platform) => Some(platform),
            None => Self::platform_from_node_version(&manifest_file),
        };

        Ok(Project {
            manifest_file,
            workspace_manifests,
            dependencies,
            platform,
        })
    }

    /// Returns a Node.js version from .node_version_file
    fn platform_from_node_version(manifest_file: &Path) -> Option<PlatformSpec> {
        // project path without package.json
        let project_path = manifest_file.parent()?;

        match std::fs::read_to_string(project_path.join(".node_version")) {
            Ok(version) => match Version::parse(version) {
                Ok(node) => Some(PlatformSpec {
                    node,
                    yarn: None,
                    npm: None,
                    pnpm: None,
                }),
                Err(_) => None,
            },
            Err(_) => None,
        }
    }

    /// Returns a reference to the manifest file for the current project
    pub fn manifest_file(&self) -> &Path {
        &self.manifest_file
    }

    /// Returns an iterator of paths to all of the workspace roots
    pub fn workspace_roots(&self) -> impl Iterator<Item = &Path> {
        // Invariant: self.manifest_file and self.extensions will only contain paths to files that we successfully loaded
        once(&self.manifest_file)
            .chain(self.workspace_manifests.iter())
            .map(|file| file.parent().expect("File paths always have a parent"))
    }

    /// Returns a reference to the Project's `PlatformSpec`, if available
    pub fn platform(&self) -> Option<&PlatformSpec> {
        self.platform.as_ref()
    }

    /// Returns true if the project dependency map contains the specified dependency
    pub fn has_direct_dependency(&self, dependency: &str) -> bool {
        self.dependencies.contains_key(dependency)
    }

    /// Returns true if the input binary name is a direct dependency of the input project
    pub fn has_direct_bin(&self, bin_name: &OsStr) -> Fallible<bool> {
        if let Some(name) = bin_name.to_str() {
            let config_path = volta_home()?.default_tool_bin_config(name);

            return match BinConfig::from_file_if_exists(config_path)? {
                None => Ok(false),
                Some(config) => Ok(self.has_direct_dependency(&config.package)),
            };
        }
        Ok(false)
    }

    /// Searches the project roots to find the path to a project-local binary file
    pub fn find_bin<P: AsRef<Path>>(&self, bin_name: P) -> Option<PathBuf> {
        self.workspace_roots().find_map(|root| {
            let mut bin_path = root.join("node_modules");
            bin_path.push(".bin");
            bin_path.push(&bin_name);

            if bin_path.is_file() {
                Some(bin_path)
            } else {
                None
            }
        })
    }

    /// Yarn projects that are using PnP or pnpm linker need to use yarn run.
    // (project uses Yarn berry if 'yarnrc.yml' exists, uses PnP if '.pnp.js' or '.pnp.cjs' exist)
    pub fn needs_yarn_run(&self) -> bool {
        self.platform()
            .is_some_and(|platform| platform.yarn.is_some())
            && self.workspace_roots().any(|x| {
                x.join(".yarnrc.yml").exists()
                    || x.join(".pnp.cjs").exists()
                    || x.join(".pnp.js").exists()
            })
    }

    /// Pins the Node version in this project's manifest file
    pub fn pin_node(&mut self, version: Version) -> Fallible<()> {
        update_manifest(&self.manifest_file, ManifestKey::Node, Some(&version))?;

        if let Some(platform) = self.platform.as_mut() {
            platform.node = version;
        } else {
            self.platform = Some(PlatformSpec {
                node: version,
                npm: None,
                pnpm: None,
                yarn: None,
            });
        }

        Ok(())
    }

    /// Pins the npm version in this project's manifest file
    pub fn pin_npm(&mut self, version: Option<Version>) -> Fallible<()> {
        if let Some(platform) = self.platform.as_mut() {
            update_manifest(&self.manifest_file, ManifestKey::Npm, version.as_ref())?;

            platform.npm = version;

            Ok(())
        } else {
            Err(ErrorKind::NoPinnedNodeVersion { tool: "npm".into() }.into())
        }
    }

    /// Pins the pnpm version in this project's manifest file
    pub fn pin_pnpm(&mut self, version: Option<Version>) -> Fallible<()> {
        if let Some(platform) = self.platform.as_mut() {
            update_manifest(&self.manifest_file, ManifestKey::Pnpm, version.as_ref())?;

            platform.pnpm = version;

            Ok(())
        } else {
            Err(ErrorKind::NoPinnedNodeVersion {
                tool: "pnpm".into(),
            }
            .into())
        }
    }

    /// Pins the Yarn version in this project's manifest file
    pub fn pin_yarn(&mut self, version: Option<Version>) -> Fallible<()> {
        if let Some(platform) = self.platform.as_mut() {
            update_manifest(&self.manifest_file, ManifestKey::Yarn, version.as_ref())?;

            platform.yarn = version;

            Ok(())
        } else {
            Err(ErrorKind::NoPinnedNodeVersion {
                tool: "Yarn".into(),
            }
            .into())
        }
    }
}

fn is_node_root(dir: &Path) -> bool {
    dir.join("package.json").exists()
}

fn is_node_modules(dir: &Path) -> bool {
    dir.file_name().is_some_and(|tail| tail == "node_modules")
}

fn is_dependency(dir: &Path) -> bool {
    dir.parent().is_some_and(is_node_modules)
}

fn is_project_root(dir: &Path) -> bool {
    is_node_root(dir) && !is_dependency(dir)
}

/// Starts at `base_dir` and walks up the directory tree until a package.json file is found
pub(crate) fn find_closest_root(mut dir: PathBuf) -> Option<PathBuf> {
    while !is_project_root(&dir) {
        if !dir.pop() {
            return None;
        }
    }

    Some(dir)
}

struct PartialPlatform {
    node: Option<Version>,
    npm: Option<Version>,
    pnpm: Option<Version>,
    yarn: Option<Version>,
}

impl PartialPlatform {
    fn merge(self, other: PartialPlatform) -> PartialPlatform {
        PartialPlatform {
            node: self.node.or(other.node),
            npm: self.npm.or(other.npm),
            pnpm: self.pnpm.or(other.pnpm),
            yarn: self.yarn.or(other.yarn),
        }
    }
}

impl TryFrom<PartialPlatform> for PlatformSpec {
    type Error = VoltaError;

    fn try_from(partial: PartialPlatform) -> Fallible<PlatformSpec> {
        let node = partial.node.ok_or(ErrorKind::NoProjectNodeInManifest)?;

        Ok(PlatformSpec {
            node,
            npm: partial.npm,
            pnpm: partial.pnpm,
            yarn: partial.yarn,
        })
    }
}
