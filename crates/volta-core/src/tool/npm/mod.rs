use std::fmt::{self, Display};

use super::node::load_default_npm_version;
use super::{
    check_fetched, check_shim_reachable, debug_already_fetched, info_fetched, info_installed,
    info_pinned, info_project_version, FetchStatus, Tool,
};
use crate::error::{Context, ErrorKind, Fallible};
use crate::inventory::npm_available;
use crate::session::Session;
use crate::style::{success_prefix, tool_version};
use crate::sync::VoltaLock;
use log::info;
use node_semver::Version;

mod fetch;
mod resolve;

pub use resolve::resolve;

/// The Tool implementation for fetching and installing npm
pub struct Npm {
    pub(super) version: Version,
}

impl Npm {
    pub fn new(version: Version) -> Self {
        Npm { version }
    }

    pub fn archive_basename(version: &str) -> String {
        format!("npm-{}", version)
    }

    pub fn archive_filename(version: &str) -> String {
        format!("{}.tgz", Npm::archive_basename(version))
    }

    pub(crate) fn ensure_fetched(&self, session: &mut Session) -> Fallible<()> {
        match check_fetched(|| npm_available(&self.version))? {
            FetchStatus::AlreadyFetched => {
                debug_already_fetched(self);
                Ok(())
            }
            FetchStatus::FetchNeeded(_lock) => fetch::fetch(&self.version, session.hooks()?.npm()),
        }
    }
}

impl Tool for Npm {
    fn fetch(self: Box<Self>, session: &mut Session) -> Fallible<()> {
        self.ensure_fetched(session)?;

        info_fetched(self);
        Ok(())
    }
    fn install(self: Box<Self>, session: &mut Session) -> Fallible<()> {
        // Acquire a lock on the Volta directory, if possible, to prevent concurrent changes
        let _lock = VoltaLock::acquire();
        self.ensure_fetched(session)?;

        session
            .toolchain_mut()?
            .set_active_npm(Some(self.version.clone()))?;

        info_installed(&self);
        check_shim_reachable("npm");

        if let Ok(Some(project)) = session.project_platform() {
            if let Some(npm) = &project.npm {
                info_project_version(tool_version("npm", npm), &self);
            }
        }
        Ok(())
    }
    fn pin(self: Box<Self>, session: &mut Session) -> Fallible<()> {
        if session.project()?.is_some() {
            self.ensure_fetched(session)?;

            // Note: We know this will succeed, since we checked above
            let project = session.project_mut()?.unwrap();
            project.pin_npm(Some(self.version.clone()))?;

            info_pinned(self);
            Ok(())
        } else {
            Err(ErrorKind::NotInPackage.into())
        }
    }
    fn uninstall(self: Box<Self>, _session: &mut Session) -> Fallible<()> {
        Err(ErrorKind::Unimplemented {
            feature: "Uninstalling npm".into(),
        }
        .into())
    }
}

impl Display for Npm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&tool_version("npm", &self.version))
    }
}

/// The Tool implementation for setting npm to the version bundled with Node
pub struct BundledNpm;

impl Tool for BundledNpm {
    fn fetch(self: Box<Self>, _session: &mut Session) -> Fallible<()> {
        info!("Bundled npm is included with Node, use `volta fetch node` to fetch Node");
        Ok(())
    }

    fn install(self: Box<Self>, session: &mut Session) -> Fallible<()> {
        let toolchain = session.toolchain_mut()?;

        toolchain.set_active_npm(None)?;

        let bundled_version = match toolchain.platform() {
            Some(platform) => {
                let version = load_default_npm_version(&platform.node).with_context(|| {
                    ErrorKind::NoBundledNpm {
                        command: "install".into(),
                    }
                })?;
                version.to_string()
            }
            None => {
                return Err(ErrorKind::NoBundledNpm {
                    command: "install".into(),
                }
                .into());
            }
        };

        info!(
            "{} set bundled npm (currently {}) as default",
            success_prefix(),
            bundled_version
        );

        Ok(())
    }

    fn pin(self: Box<Self>, session: &mut Session) -> Fallible<()> {
        match session.project_mut()? {
            Some(project) => {
                project.pin_npm(None)?;

                let bundled_version = match project.platform() {
                    Some(platform) => {
                        let version =
                            load_default_npm_version(&platform.node).with_context(|| {
                                ErrorKind::NoBundledNpm {
                                    command: "pin".into(),
                                }
                            })?;
                        version.to_string()
                    }
                    None => {
                        return Err(ErrorKind::NoBundledNpm {
                            command: "pin".into(),
                        }
                        .into());
                    }
                };

                info!(
                    "{} set package.json to use bundled npm (currently {})",
                    success_prefix(),
                    bundled_version
                );

                Ok(())
            }
            None => Err(ErrorKind::NotInPackage.into()),
        }
    }

    fn uninstall(self: Box<Self>, _session: &mut Session) -> Fallible<()> {
        Err(ErrorKind::Unimplemented {
            feature: "Uninstalling bundled npm".into(),
        }
        .into())
    }
}

impl Display for BundledNpm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&tool_version("npm", "bundled"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_npm_archive_basename() {
        assert_eq!(Npm::archive_basename("1.2.3"), "npm-1.2.3");
    }

    #[test]
    fn test_npm_archive_filename() {
        assert_eq!(Npm::archive_filename("1.2.3"), "npm-1.2.3.tgz");
    }
}
