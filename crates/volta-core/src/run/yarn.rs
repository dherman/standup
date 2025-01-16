use std::env;
use std::ffi::OsString;

use super::executor::{Executor, ToolCommand, ToolKind};
use super::parser::CommandArg;
use super::{debug_active_image, debug_no_platform, RECURSION_ENV_VAR};
use crate::error::{ErrorKind, Fallible};
use crate::platform::{Platform, Source, System};
use crate::session::{ActivityKind, Session};

/// Build an `Executor` for Yarn
///
/// If the command is a global add or remove and we have a default platform available, then we will
/// use custom logic to ensure that the package is correctly installed / uninstalled in the Volta
/// directory.
///
/// If the command is _not_ a global add / remove or we don't have a default platform, then
/// we will allow Yarn to execute the command as usual.
pub(super) fn command(args: &[OsString], session: &mut Session) -> Fallible<Executor> {
    session.add_event_start(ActivityKind::Yarn);
    // Don't re-evaluate the context or global install interception if this is a recursive call
    let platform = match env::var_os(RECURSION_ENV_VAR) {
        Some(_) => None,
        None => {
            if let CommandArg::Global(cmd) = CommandArg::for_yarn(args) {
                // For globals, only intercept if the default platform exists
                if let Some(default_platform) = session.default_platform()? {
                    return cmd.executor(default_platform);
                }
            }

            Platform::current(session)?
        }
    };

    Ok(ToolCommand::new("yarn", args, platform, ToolKind::Yarn).into())
}

/// Determine the execution context (PATH and failure error message) for Yarn
pub(super) fn execution_context(
    platform: Option<Platform>,
    session: &mut Session,
) -> Fallible<(OsString, ErrorKind)> {
    match platform {
        Some(plat) => {
            let on_failure = platform_yarn_error(&plat);

            let image = plat.checkout(session)?;
            let path = image.path()?;
            debug_active_image(&image);

            Ok((path, on_failure))
        }
        None => {
            let path = System::path()?;
            debug_no_platform();
            Ok((path, ErrorKind::NoPlatform))
        }
    }
}

fn platform_yarn_error(platform: &Platform) -> ErrorKind {
    match &platform.yarn {
        Some(_) => ErrorKind::BinaryExecError,
        None => match platform.node.source {
            Source::Project => ErrorKind::NoProjectYarn,
            Source::Default | Source::Binary => ErrorKind::NoDefaultYarn,
            Source::CommandLine => ErrorKind::NoCommandLineYarn,
        },
    }
}
