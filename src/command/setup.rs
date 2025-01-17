use log::info;
use volta_core::error::{ExitCode, Fallible};
use volta_core::layout::volta_home;
use volta_core::session::{ActivityKind, Session};
use volta_core::shim::regenerate_shims_for_dir;
use volta_core::style::success_prefix;

use crate::command::Command;

#[derive(clap::Args)]
pub(crate) struct Setup {}

impl Command for Setup {
    fn run(self, session: &mut Session) -> Fallible<ExitCode> {
        session.add_event_start(ActivityKind::Setup);

        os::setup_environment()?;
        regenerate_shims_for_dir(volta_home()?.shim_dir())?;

        info!(
            "{} Setup complete. Open a new terminal to start using Volta!",
            success_prefix()
        );

        session.add_event_end(ActivityKind::Setup, ExitCode::Success);
        Ok(ExitCode::Success)
    }
}

#[cfg(unix)]
mod os {
    use std::env;
    use std::fs::File;
    use std::io::{self, BufRead, BufReader, Write};
    use std::path::{Path, PathBuf};

    use log::{debug, warn};
    use volta_core::error::{ErrorKind, Fallible};
    use volta_core::layout::volta_home;

    pub fn setup_environment() -> Fallible<()> {
        let home = volta_home()?;
        let formatted_home = format_home(home.root());

        // Don't update the user's shell config files if VOLTA_HOME and PATH already contain what we need.
        let home_in_path = match env::var_os("PATH") {
            Some(paths) => env::split_paths(&paths).find(|p| p == home.shim_dir()),
            None => None,
        };

        if env::var_os("VOLTA_HOME").is_some() && home_in_path.is_some() {
            debug!(
                "Skipping dot-file modification as VOLTA_HOME is set, and included in the PATH."
            );
            return Ok(());
        }

        debug!("Searching for profiles to update");
        let profiles = determine_profiles()?;

        let found_profile = profiles.into_iter().fold(false, |prev, profile| {
            let contents = read_profile_without_volta(&profile).unwrap_or_default();

            let write_profile = match profile.extension() {
                Some(ext) if ext == "fish" => write_profile_fish,
                _ => write_profile_sh,
            };

            match write_profile(&profile, contents, &formatted_home) {
                Ok(()) => true,
                Err(err) => {
                    warn!(
                        "Found profile script, but could not modify it: {}",
                        profile.display()
                    );
                    debug!("Profile modification error: {}", err);
                    prev
                }
            }
        });

        if found_profile {
            Ok(())
        } else {
            Err(ErrorKind::NoShellProfile {
                env_profile: String::new(),
                bin_dir: home.shim_dir().to_owned(),
            }
            .into())
        }
    }

    /// Returns a list of profile files to modify / create.
    ///
    /// Any file in the list should be created if it doesn't already exist
    fn determine_profiles() -> Fallible<Vec<PathBuf>> {
        let home_dir = dirs::home_dir().ok_or(ErrorKind::NoHomeEnvironmentVar)?;
        let shell = env::var("SHELL").unwrap_or_else(|_| String::new());
        // Always include `~/.profile`
        let mut profiles = vec![home_dir.join(".profile")];

        // PROFILE environment variable, if set
        if let Ok(profile_env) = env::var("PROFILE") {
            if !profile_env.is_empty() {
                profiles.push(profile_env.into());
            }
        }

        add_zsh_profile(&home_dir, &shell, &mut profiles);
        add_bash_profiles(&home_dir, &shell, &mut profiles);
        add_fish_profile(&home_dir, &shell, &mut profiles);

        Ok(profiles)
    }

    /// Add zsh profile script, if necessary
    fn add_zsh_profile(home_dir: &Path, shell: &str, profiles: &mut Vec<PathBuf>) {
        let zdotdir_env = env::var("ZDOTDIR").unwrap_or_else(|_| String::new());
        let zdotdir = if zdotdir_env.is_empty() {
            home_dir
        } else {
            Path::new(&zdotdir_env)
        };

        let zshenv = zdotdir.join(".zshenv");

        let zshrc = zdotdir.join(".zshrc");

        if shell.contains("zsh") || zshenv.exists() {
            profiles.push(zshenv);
        } else if zshrc.exists() {
            profiles.push(zshrc);
        }
    }

    /// Add bash profile scripts, if necessary
    ///
    /// Note: We only add the bash scripts if they already exist, as creating new files can impact
    /// the processing of existing files in bash (e.g. preventing ~/.profile from being loaded)
    fn add_bash_profiles(home_dir: &Path, shell: &str, profiles: &mut Vec<PathBuf>) {
        let mut bash_added = false;

        let bashrc = home_dir.join(".bashrc");
        if bashrc.exists() {
            bash_added = true;
            profiles.push(bashrc);
        }

        let bash_profile = home_dir.join(".bash_profile");
        if bash_profile.exists() {
            bash_added = true;
            profiles.push(bash_profile);
        }

        if shell.contains("bash") && !bash_added {
            let suggested_bash_profile = if cfg!(target_os = "macos") {
                "~/.bash_profile"
            } else {
                "~/.bashrc"
            };

            warn!(
                "We detected that you are using bash, however we couldn't find any bash profile scripts.
If you run into problems running Volta, create {} and run `volta setup` again.",
                suggested_bash_profile
            );
        }
    }

    /// Add fish profile scripts, if necessary
    fn add_fish_profile(home_dir: &Path, shell: &str, profiles: &mut Vec<PathBuf>) {
        let fish_config = home_dir.join(".config/fish/config.fish");

        if shell.contains("fish") || fish_config.exists() {
            profiles.push(fish_config);
        }
    }

    fn read_profile_without_volta(path: &Path) -> Option<String> {
        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);

        reader
            .lines()
            .filter(|line_result| match line_result {
                Ok(line) if !line.contains("VOLTA") => true,
                Ok(_) => false,
                Err(_) => true,
            })
            .collect::<io::Result<Vec<String>>>()
            .map(|lines| lines.join("\n"))
            .ok()
    }

    fn format_home(volta_home: &Path) -> String {
        if let Some(home_dir) = env::var_os("HOME") {
            if let Ok(suffix) = volta_home.strip_prefix(home_dir) {
                // If the HOME environment variable is set _and_ the proposed VOLTA_HOME starts
                // with that value, use $HOME when writing the profile scripts
                return format!("$HOME/{}", suffix.display());
            }
        }

        volta_home.display().to_string()
    }

    fn write_profile_sh(path: &Path, contents: String, volta_home: &str) -> io::Result<()> {
        let mut file = File::create(path)?;
        write!(
            file,
            "{}\nexport VOLTA_HOME=\"{}\"\nexport PATH=\"$VOLTA_HOME/bin:$PATH\"\n",
            contents, volta_home,
        )
    }

    fn write_profile_fish(path: &Path, contents: String, volta_home: &str) -> io::Result<()> {
        let mut file = File::create(path)?;
        write!(
            file,
            "{}\nset -gx VOLTA_HOME \"{}\"\nset -gx PATH \"$VOLTA_HOME/bin\" $PATH\n",
            contents, volta_home,
        )
    }
}

#[cfg(windows)]
mod os {
    use log::debug;
    use volta_core::error::{Context, ErrorKind, Fallible};
    use volta_core::layout::volta_home;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    pub fn setup_environment() -> Fallible<()> {
        let shim_dir = volta_home()?.shim_dir().to_string_lossy().to_string();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let env = hkcu
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .with_context(|| ErrorKind::ReadUserPathError)?;
        let path: String = env
            .get_value("Path")
            .with_context(|| ErrorKind::ReadUserPathError)?;

        if !path.contains(&shim_dir) {
            let path = format!("{};{}", shim_dir, path);
            debug!("Modifying User Path to: {}", path);

            // see https://superuser.com/a/387625
            env.set_value("Path", &path)
                .with_context(|| ErrorKind::WriteUserPathError)?;
        }

        Ok(())
    }
}
