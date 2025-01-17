use std::fmt::{self, Display};

use super::{
    check_fetched, check_shim_reachable, debug_already_fetched, info_fetched, info_installed,
    info_pinned, info_project_version, FetchStatus, Tool,
};
use crate::error::{Context, ErrorKind, Fallible};
use crate::fs::{dir_entry_match, ok_if_not_found, remove_dir_if_exists, remove_file_if_exists};
use crate::inventory::node_available;
use crate::layout::volta_home;
use crate::session::Session;
use crate::style::{note_prefix, success_prefix, tool_version};
use crate::sync::VoltaLock;
use cfg_if::cfg_if;
use log::{info, warn};
use node_semver::Version;

mod fetch;
mod metadata;
mod resolve;

pub use fetch::load_default_npm_version;
pub use resolve::resolve;

cfg_if! {
    if #[cfg(all(target_os = "windows", target_arch = "x86"))] {
        /// The OS component of a Node distro filename
        pub const NODE_DISTRO_OS: &str = "win";
        /// The architecture component of a Node distro filename
        pub const NODE_DISTRO_ARCH: &str = "x86";
        /// The extension for Node distro files
        pub const NODE_DISTRO_EXTENSION: &str = "zip";
        /// The file identifier in the Node index `files` array
        pub const NODE_DISTRO_IDENTIFIER: &str = "win-x86-zip";
    } else if #[cfg(all(target_os = "windows", target_arch = "x86_64"))] {
        /// The OS component of a Node distro filename
        pub const NODE_DISTRO_OS: &str = "win";
        /// The architecture component of a Node distro filename
        pub const NODE_DISTRO_ARCH: &str = "x64";
        /// The extension for Node distro files
        pub const NODE_DISTRO_EXTENSION: &str = "zip";
        /// The file identifier in the Node index `files` array
        pub const NODE_DISTRO_IDENTIFIER: &str = "win-x64-zip";
    } else if #[cfg(all(target_os = "windows", target_arch = "aarch64"))] {
        /// The OS component of a Node distro filename
        pub const NODE_DISTRO_OS: &str = "win";
        /// The architecture component of a Node distro filename
        pub const NODE_DISTRO_ARCH: &str = "arm64";
        /// The extension for Node distro files
        pub const NODE_DISTRO_EXTENSION: &str = "zip";
        /// The file identifier in the Node index `files` array
        pub const NODE_DISTRO_IDENTIFIER: &str = "win-arm64-zip";

        // NOTE: Node support for pre-built ARM64 binaries on Windows was added in major version 20
        // For versions prior to that, we need to fall back on the x64 binaries via emulator

        /// The fallback architecture component of a Node distro filename
        pub const NODE_DISTRO_ARCH_FALLBACK: &str = "x64";
        /// The fallback file identifier in the Node index `files` array
        pub const NODE_DISTRO_IDENTIFIER_FALLBACK: &str = "win-x64-zip";
    } else if #[cfg(all(target_os = "macos", target_arch = "x86_64"))] {
        /// The OS component of a Node distro filename
        pub const NODE_DISTRO_OS: &str = "darwin";
        /// The architecture component of a Node distro filename
        pub const NODE_DISTRO_ARCH: &str = "x64";
        /// The extension for Node distro files
        pub const NODE_DISTRO_EXTENSION: &str = "tar.gz";
        /// The file identifier in the Node index `files` array
        pub const NODE_DISTRO_IDENTIFIER: &str = "osx-x64-tar";
    } else if #[cfg(all(target_os = "macos", target_arch = "aarch64"))] {
        /// The OS component of a Node distro filename
        pub const NODE_DISTRO_OS: &str = "darwin";
        /// The architecture component of a Node distro filename
        pub const NODE_DISTRO_ARCH: &str = "arm64";
        /// The extension for Node distro files
        pub const NODE_DISTRO_EXTENSION: &str = "tar.gz";
        /// The file identifier in the Node index `files` array
        pub const NODE_DISTRO_IDENTIFIER: &str = "osx-arm64-tar";

        // NOTE: Node support for pre-built Apple Silicon binaries was added in major version 16
        // For versions prior to that, we need to fall back on the x64 binaries via Rosetta 2

        /// The fallback architecture component of a Node distro filename
        pub const NODE_DISTRO_ARCH_FALLBACK: &str = "x64";
        /// The fallback file identifier in the Node index `files` array
        pub const NODE_DISTRO_IDENTIFIER_FALLBACK: &str = "osx-x64-tar";
    } else if #[cfg(all(target_os = "linux", target_arch = "x86_64"))] {
        /// The OS component of a Node distro filename
        pub const NODE_DISTRO_OS: &str = "linux";
        /// The architecture component of a Node distro filename
        pub const NODE_DISTRO_ARCH: &str = "x64";
        /// The extension for Node distro files
        pub const NODE_DISTRO_EXTENSION: &str = "tar.gz";
        /// The file identifier in the Node index `files` array
        pub const NODE_DISTRO_IDENTIFIER: &str = "linux-x64";
    } else if #[cfg(all(target_os = "linux", target_arch = "aarch64"))] {
        /// The OS component of a Node distro filename
        pub const NODE_DISTRO_OS: &str = "linux";
        /// The architecture component of a Node distro filename
        pub const NODE_DISTRO_ARCH: &str = "arm64";
        /// The extension for Node distro files
        pub const NODE_DISTRO_EXTENSION: &str = "tar.gz";
        /// The file identifier in the Node index `files` array
        pub const NODE_DISTRO_IDENTIFIER: &str = "linux-arm64";
    } else if #[cfg(all(target_os = "linux", target_arch = "arm"))] {
        /// The OS component of a Node distro filename
        pub const NODE_DISTRO_OS: &str = "linux";
        /// The architecture component of a Node distro filename
        pub const NODE_DISTRO_ARCH: &str = "armv7l";
        /// The extension for Node distro files
        pub const NODE_DISTRO_EXTENSION: &str = "tar.gz";
        /// The file identifier in the Node index `files` array
        pub const NODE_DISTRO_IDENTIFIER: &str = "linux-armv7l";
    } else {
        compile_error!("Unsuppored operating system + architecture combination");
    }
}

/// A full Node version including not just the version of Node itself
/// but also the specific version of npm installed globally with that
/// Node installation.
#[derive(Clone, Debug)]
pub struct NodeVersion {
    /// The version of Node itself.
    pub runtime: Version,
    /// The npm version globally installed with the Node distro.
    pub npm: Version,
}

impl Display for NodeVersion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} (with {})",
            tool_version("node", &self.runtime),
            tool_version("npm", &self.npm)
        )
    }
}

/// The Tool implementation for fetching and installing Node
pub struct Node {
    pub(super) version: Version,
}

impl Node {
    pub fn new(version: Version) -> Self {
        Node { version }
    }

    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "aarch64")
    )))]
    pub fn archive_basename(version: &Version) -> String {
        format!("node-v{}-{}-{}", version, NODE_DISTRO_OS, NODE_DISTRO_ARCH)
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub fn archive_basename(version: &Version) -> String {
        // Note: Node began shipping pre-built binaries for Apple Silicon with Major version 16
        // Prior to that, we need to fall back on the x64 binaries
        format!(
            "node-v{}-{}-{}",
            version,
            NODE_DISTRO_OS,
            if version.major >= 16 {
                NODE_DISTRO_ARCH
            } else {
                NODE_DISTRO_ARCH_FALLBACK
            }
        )
    }

    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    pub fn archive_basename(version: &Version) -> String {
        // Note: Node began shipping pre-built binaries for Windows ARM with Major version 20
        // Prior to that, we need to fall back on the x64 binaries
        format!(
            "node-v{}-{}-{}",
            version,
            NODE_DISTRO_OS,
            if version.major >= 20 {
                NODE_DISTRO_ARCH
            } else {
                NODE_DISTRO_ARCH_FALLBACK
            }
        )
    }

    pub fn archive_filename(version: &Version) -> String {
        format!(
            "{}.{}",
            Node::archive_basename(version),
            NODE_DISTRO_EXTENSION
        )
    }

    pub(crate) fn ensure_fetched(&self, session: &mut Session) -> Fallible<NodeVersion> {
        match check_fetched(|| node_available(&self.version))? {
            FetchStatus::AlreadyFetched => {
                debug_already_fetched(self);
                let npm = fetch::load_default_npm_version(&self.version)?;

                Ok(NodeVersion {
                    runtime: self.version.clone(),
                    npm,
                })
            }
            FetchStatus::FetchNeeded(_lock) => fetch::fetch(&self.version, session.hooks()?.node()),
        }
    }
}

impl Tool for Node {
    fn fetch(self: Box<Self>, session: &mut Session) -> Fallible<()> {
        let node_version = self.ensure_fetched(session)?;

        info_fetched(node_version);
        Ok(())
    }
    fn install(self: Box<Self>, session: &mut Session) -> Fallible<()> {
        // Acquire a lock on the Volta directory, if possible, to prevent concurrent changes
        let _lock = VoltaLock::acquire();
        let node_version = self.ensure_fetched(session)?;

        let default_toolchain = session.toolchain_mut()?;
        default_toolchain.set_active_node(&self.version)?;

        // If the user has a default version of `npm`, we shouldn't show the "(with npm@X.Y.ZZZ)" text in the success message
        // Instead we should check if the bundled version is higher than the default and inform the user
        // Note: The previous line ensures that there will be a default platform
        if let Some(default_npm) = &default_toolchain.platform().unwrap().npm {
            info_installed(&self); // includes node version

            if node_version.npm > *default_npm {
                info!("{} this version of Node includes {}, which is higher than your default version ({}).
      To use the version included with Node, run `volta install npm@bundled`",
                    note_prefix(),
                    tool_version("npm", node_version.npm),
                    default_npm.to_string()
                );
            }
        } else {
            info_installed(node_version); // includes node and npm version
        }

        check_shim_reachable("node");

        if let Ok(Some(project)) = session.project_platform() {
            info_project_version(tool_version("node", &project.node), &self);
        }

        Ok(())
    }
    fn pin(self: Box<Self>, session: &mut Session) -> Fallible<()> {
        if session.project()?.is_some() {
            let node_version = self.ensure_fetched(session)?;

            // Note: We know this will succeed, since we checked above
            let project = session.project_mut()?.unwrap();
            project.pin_node(self.version.clone())?;

            // If the user has a pinned version of `npm`, we shouldn't show the "(with npm@X.Y.ZZZ)" text in the success message
            // Instead we should check if the bundled version is higher than the pinned and inform the user
            // Note: The pin operation guarantees there will be a platform
            if let Some(pinned_npm) = &project.platform().unwrap().npm {
                info_pinned(self); // includes node version

                if node_version.npm > *pinned_npm {
                    info!("{} this version of Node includes {}, which is higher than your pinned version ({}).
      To use the version included with Node, run `volta pin npm@bundled`",
                        note_prefix(),
                        tool_version("npm", node_version.npm),
                        pinned_npm.to_string()
                    );
                }
            } else {
                info_pinned(node_version); // includes node and npm version
            }

            Ok(())
        } else {
            Err(ErrorKind::NotInPackage.into())
        }
    }
    fn uninstall(self: Box<Self>, _session: &mut Session) -> Fallible<()> {
        let home = volta_home()?;
        // Acquire a lock on the Volta directory, if possible, to prevent concurrent changes
        let _lock: Result<VoltaLock, crate::error::VoltaError> = VoltaLock::acquire();

        let node_dir = home.node_image_root_dir().join(self.version.to_string());

        dir_entry_match(home.node_inventory_dir(), |entry| {
            let path = entry.path();

            if path.is_file() {
                match path.file_name().and_then(|name| name.to_str()) {
                    Some(file_name) if file_name.contains(&self.version.to_string()) => Some(path),
                    _ => None,
                }
            } else {
                None
            }
        })
        .or_else(ok_if_not_found)
        .with_context(|| ErrorKind::ReadDirError {
            dir: home.node_inventory_dir().to_path_buf(),
        })
        .map(|files| {
            files.iter().for_each(|file| {
                remove_file_if_exists(file);
            })
        });

        if node_dir.exists() {
            remove_dir_if_exists(&node_dir)?;
            info!("{} 'node@{}' uninstalled", success_prefix(), self.version);
        } else {
            warn!("No version 'node@{}' found to uninstall", self.version);
        }

        Ok(())
    }
}

impl Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&tool_version("node", &self.version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_archive_basename() {
        assert_eq!(
            Node::archive_basename(&Version::parse("20.2.3").unwrap()),
            format!("node-v20.2.3-{}-{}", NODE_DISTRO_OS, NODE_DISTRO_ARCH)
        );
    }

    #[test]
    fn test_node_archive_filename() {
        assert_eq!(
            Node::archive_filename(&Version::parse("20.2.3").unwrap()),
            format!(
                "node-v20.2.3-{}-{}.{}",
                NODE_DISTRO_OS, NODE_DISTRO_ARCH, NODE_DISTRO_EXTENSION
            )
        );
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn test_fallback_node_archive_basename() {
        assert_eq!(
            Node::archive_basename(&Version::parse("15.2.3").unwrap()),
            format!(
                "node-v15.2.3-{}-{}",
                NODE_DISTRO_OS, NODE_DISTRO_ARCH_FALLBACK
            )
        );
    }

    #[test]
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    fn test_fallback_node_archive_basename() {
        assert_eq!(
            Node::archive_basename(&Version::parse("19.2.3").unwrap()),
            format!(
                "node-v19.2.3-{}-{}",
                NODE_DISTRO_OS, NODE_DISTRO_ARCH_FALLBACK
            )
        );
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn test_fallback_node_archive_filename() {
        assert_eq!(
            Node::archive_filename(&Version::parse("15.2.3").unwrap()),
            format!(
                "node-v15.2.3-{}-{}.{}",
                NODE_DISTRO_OS, NODE_DISTRO_ARCH_FALLBACK, NODE_DISTRO_EXTENSION
            )
        );
    }

    #[test]
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    fn test_fallback_node_archive_filename() {
        assert_eq!(
            Node::archive_filename(&Version::parse("19.2.3").unwrap()),
            format!(
                "node-v19.2.3-{}-{}.{}",
                NODE_DISTRO_OS, NODE_DISTRO_ARCH_FALLBACK, NODE_DISTRO_EXTENSION
            )
        );
    }
}
