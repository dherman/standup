use std::ffi::OsStr;
use std::process::Command;

use cfg_if::cfg_if;
use log::debug;

use crate::error::Fallible;

cfg_if! {
    if #[cfg(windows)] {
        pub fn create_command<E>(exe: E) -> Command
        where
            E: AsRef<OsStr>
        {
            // Several of the node utilities are implemented as `.bat` or `.cmd` files
            // When executing those files with `Command`, we need to call them with:
            //    cmd.exe /C <COMMAND> <ARGUMENTS>
            // Instead of: <COMMAND> <ARGUMENTS>
            // See: https://github.com/rust-lang/rust/issues/42791 For a longer discussion
            let mut command = Command::new("cmd.exe");
            command.arg("/C");
            command.arg(exe);
            command
        }
    } else {
        pub fn create_command<E>(exe: E) -> Command
        where
            E: AsRef<OsStr>
        {
            Command::new(exe)
        }
    }
}

/// Rebuild command against given PATH
///
/// On Windows, we need to explicitly use an absolute path to the executable,
/// otherwise the executable will not be located properly, even if we've set the PATH.
/// see: https://github.com/rust-lang/rust/issues/37519
///
/// This function will try to find the executable in the given path and rebuild
/// the command with the absolute path to the executable.
pub fn rebuild_command<S: AsRef<OsStr>>(command: Command, path: S) -> Fallible<Command> {
    debug!("PATH: {}", path.as_ref().to_string_lossy());

    #[cfg(not(windows))]
    {
        let mut command = command;
        command.env("PATH", path.as_ref());
        Ok(command)
    }

    #[cfg(windows)]
    {
        let args = command.get_args().collect::<Vec<_>>();
        //          cmd /c <name> [...other]
        // args_idx     0  1      2..
        let name = args.get(1).expect("A command always has a name");

        if let Ok(mut paths) = which::which_in_global(name, Some(&path)) {
            if let Some(exe) = paths.next() {
                let mut new_command = create_command(exe);
                let envs = command
                    .get_envs()
                    .filter(|(_, v)| v.is_some())
                    .map(|(k, v)| (k, v.unwrap()))
                    .collect::<Vec<_>>();

                // The args will be the command name and any additional args.
                new_command.args(&args[2..]);
                new_command.envs(envs);
                new_command.env("PATH", path.as_ref());

                return Ok(new_command);
            }
        }

        Err(crate::error::ErrorKind::BinaryNotFound {
            name: name.to_string_lossy().to_string(),
        }
        .into())
    }
}
