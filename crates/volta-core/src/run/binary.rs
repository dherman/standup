use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use super::executor::{Executor, ToolCommand, ToolKind};
use super::{debug_active_image, debug_no_platform};
use crate::error::{Context, ErrorKind, Fallible};
use crate::layout::volta_home;
use crate::platform::{Platform, Sourced, System};
use crate::session::Session;
use crate::style::{note_prefix, tool_version};
use crate::tool::package::BinConfig;
use log::{debug, info};

/// Determine the correct command to run for a 3rd-party binary
///
/// Will detect if we should delegate to the project-local version or use the default version
pub(super) fn command(exe: &OsStr, args: &[OsString], session: &mut Session) -> Fallible<Executor> {
    let bin = exe.to_string_lossy().to_string();

    fn create_default_tool_executor(
        default_tool: DefaultBinary,
        bin: String,
        args: &[OsString],
    ) -> Fallible<Executor> {
        let mut command = ToolCommand::new(
            default_tool.bin_path,
            args,
            Some(default_tool.platform),
            ToolKind::DefaultBinary(bin),
        );
        command.env("NODE_PATH", shared_module_path()?);
        Ok(command.into())
    }

    // First try to use the project toolchain
    if let Some(project) = session.project()? {
        // Check if the executable is a direct dependency
        if project.has_direct_bin(exe)? {
            match project.find_bin(exe) {
                Some(path_to_bin) => {
                    debug!("Found {} in project at '{}'", bin, path_to_bin.display());

                    let platform = Platform::current(session)?;
                    return Ok(ToolCommand::new(
                        path_to_bin,
                        args,
                        platform,
                        ToolKind::ProjectLocalBinary(bin),
                    )
                    .into());
                }
                None => {
                    if project.needs_yarn_run() {
                        debug!(
                            "Project needs to use yarn to run command, calling {} with 'yarn'",
                            bin
                        );
                        let platform = Platform::current(session)?;
                        let mut exe_and_args = vec![exe.to_os_string()];
                        exe_and_args.extend_from_slice(args);
                        return Ok(ToolCommand::new(
                            "yarn",
                            exe_and_args,
                            platform,
                            ToolKind::Yarn,
                        )
                        .into());
                    } else if let Some(default_tool) = DefaultBinary::from_name(exe, session)? {
                        // if local binary not found, use global version
                        info!(
                            "{} Local binary {} not found, using global {} from:\n    {}\n",
                            note_prefix(),
                            bin,
                            tool_version(&bin, ""),
                            default_tool.bin_path.display()
                        );

                        return create_default_tool_executor(default_tool, bin, args);
                    } else {
                        return Err(ErrorKind::ProjectLocalBinaryNotFound {
                            command: exe.to_string_lossy().to_string(),
                        }
                        .into());
                    }
                }
            }
        }
    }

    // Try to use the default toolchain
    if let Some(default_tool) = DefaultBinary::from_name(exe, session)? {
        debug!(
            "Found default {} in '{}'",
            bin,
            default_tool.bin_path.display()
        );

        return create_default_tool_executor(default_tool, bin, args);
    }

    // At this point, the binary is not known to Volta, so we have no platform to use to execute it
    // This should be rare, as anything we have a shim for should have a config file to load
    Ok(ToolCommand::new(exe, args, None, ToolKind::DefaultBinary(bin)).into())
}

/// Determine the execution context (PATH and failure error message) for a project-local binary
pub(super) fn local_execution_context(
    tool: String,
    platform: Option<Platform>,
    session: &mut Session,
) -> Fallible<(OsString, ErrorKind)> {
    match platform {
        Some(plat) => {
            let image = plat.checkout(session)?;
            let path = image.path()?;
            debug_active_image(&image);

            Ok((
                path,
                ErrorKind::ProjectLocalBinaryExecError { command: tool },
            ))
        }
        None => {
            let path = System::path()?;
            debug_no_platform();

            Ok((path, ErrorKind::NoPlatform))
        }
    }
}

/// Determine the execution context (PATH and failure error message) for a default binary
pub(super) fn default_execution_context(
    tool: String,
    platform: Option<Platform>,
    session: &mut Session,
) -> Fallible<(OsString, ErrorKind)> {
    match platform {
        Some(plat) => {
            let image = plat.checkout(session)?;
            let path = image.path()?;
            debug_active_image(&image);

            Ok((path, ErrorKind::BinaryExecError))
        }
        None => {
            let path = System::path()?;
            debug_no_platform();

            Ok((path, ErrorKind::BinaryNotFound { name: tool }))
        }
    }
}

/// Information about the location and execution context of default binaries
///
/// Fetched from the config files in the Volta directory, represents the binary that is executed
/// when the user is outside of a project that has the given bin as a dependency.
pub struct DefaultBinary {
    pub bin_path: PathBuf,
    pub platform: Platform,
}

impl DefaultBinary {
    pub fn from_config(bin_config: BinConfig, session: &mut Session) -> Fallible<Self> {
        let package_dir = volta_home()?.package_image_dir(&bin_config.package);
        let mut bin_path = bin_config.manager.binary_dir(package_dir);
        bin_path.push(&bin_config.name);

        // If the user does not have yarn set in the platform for this binary, use the default
        // This is necessary because some tools (e.g. ember-cli with the `--yarn` option) invoke `yarn`
        let yarn = match bin_config.platform.yarn {
            Some(yarn) => Some(yarn),
            None => session
                .default_platform()?
                .and_then(|plat| plat.yarn.clone()),
        };
        let platform = Platform {
            node: Sourced::with_binary(bin_config.platform.node),
            npm: bin_config.platform.npm.map(Sourced::with_binary),
            pnpm: bin_config.platform.pnpm.map(Sourced::with_binary),
            yarn: yarn.map(Sourced::with_binary),
        };

        Ok(DefaultBinary { bin_path, platform })
    }

    /// Load information about a default binary by name, if available
    ///
    /// A `None` response here means that the tool information couldn't be found. Either the tool
    /// name is not a valid UTF-8 string, or the tool config doesn't exist.
    pub fn from_name(tool_name: &OsStr, session: &mut Session) -> Fallible<Option<Self>> {
        let bin_config_file = match tool_name.to_str() {
            Some(name) => volta_home()?.default_tool_bin_config(name),
            None => return Ok(None),
        };

        match BinConfig::from_file_if_exists(bin_config_file)? {
            Some(config) => DefaultBinary::from_config(config, session).map(Some),
            None => Ok(None),
        }
    }
}

/// Determine the value for NODE_PATH, with the shared lib directory prepended
///
/// This will ensure that global bins can `require` other global libs
fn shared_module_path() -> Fallible<OsString> {
    let node_path = match env::var("NODE_PATH") {
        Ok(path) => envoy::Var::from(path),
        Err(_) => envoy::Var::from(""),
    };

    node_path
        .split()
        .prefix_entry(volta_home()?.shared_lib_root())
        .join()
        .with_context(|| ErrorKind::BuildPathError)
}
