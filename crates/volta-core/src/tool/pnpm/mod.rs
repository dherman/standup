use node_semver::Version;
use std::env;
use std::fmt::{self, Display};

use crate::error::{ErrorKind, Fallible};
use crate::inventory::pnpm_available;
use crate::session::Session;
use crate::style::tool_version;
use crate::sync::VoltaLock;
use crate::VOLTA_FEATURE_PNPM;

use super::{
    check_fetched, check_shim_reachable, debug_already_fetched, info_fetched, info_installed,
    info_pinned, info_project_version, FetchStatus, Tool,
};

mod fetch;
mod resolve;

use super::package::uninstall;
pub use resolve::resolve;

/// The Tool implementation for fetching and installing pnpm
pub struct Pnpm {
    pub(super) version: Version,
}

impl Pnpm {
    pub fn new(version: Version) -> Self {
        Pnpm { version }
    }

    pub fn archive_basename(version: &str) -> String {
        format!("pnpm-{}", version)
    }

    pub fn archive_filename(version: &str) -> String {
        format!("{}.tgz", Pnpm::archive_basename(version))
    }

    pub(crate) fn ensure_fetched(&self, session: &mut Session) -> Fallible<()> {
        match check_fetched(|| pnpm_available(&self.version))? {
            FetchStatus::AlreadyFetched => {
                debug_already_fetched(self);
                Ok(())
            }
            FetchStatus::FetchNeeded(_lock) => fetch::fetch(&self.version, session.hooks()?.pnpm()),
        }
    }
}

impl Tool for Pnpm {
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
            .set_active_pnpm(Some(self.version.clone()))?;

        info_installed(&self);
        check_shim_reachable("pnpm");

        if let Ok(Some(project)) = session.project_platform() {
            if let Some(pnpm) = &project.pnpm {
                info_project_version(tool_version("pnpm", pnpm), &self);
            }
        }
        Ok(())
    }

    fn pin(self: Box<Self>, session: &mut Session) -> Fallible<()> {
        if session.project()?.is_some() {
            self.ensure_fetched(session)?;

            // Note: We know this will succeed, since we checked above
            let project = session.project_mut()?.unwrap();
            project.pin_pnpm(Some(self.version.clone()))?;

            info_pinned(self);
            Ok(())
        } else {
            Err(ErrorKind::NotInPackage.into())
        }
    }

    fn uninstall(self: Box<Self>, _session: &mut Session) -> Fallible<()> {
        if env::var_os(VOLTA_FEATURE_PNPM).is_some() {
            Err(ErrorKind::Unimplemented {
                feature: "Uninstalling pnpm".into(),
            }
            .into())
        } else {
            uninstall("pnpm")
        }
    }
}

impl Display for Pnpm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&tool_version("pnpm", &self.version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pnpm_archive_basename() {
        assert_eq!(Pnpm::archive_basename("1.2.3"), "pnpm-1.2.3");
    }

    #[test]
    fn test_pnpm_archive_filename() {
        assert_eq!(Pnpm::archive_filename("1.2.3"), "pnpm-1.2.3.tgz");
    }
}
