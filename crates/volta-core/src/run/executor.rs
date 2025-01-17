use std::collections::HashMap;
use std::ffi::OsStr;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;
use std::process::{Command, ExitStatus};

use super::RECURSION_ENV_VAR;
use crate::command::create_command;
use crate::error::{Context, ErrorKind, Fallible};
use crate::layout::volta_home;
use crate::platform::{CliPlatform, Platform, System};
use crate::session::Session;
use crate::signal::pass_control_to_shim;
use crate::style::{note_prefix, tool_version};
use crate::sync::VoltaLock;
use crate::tool::package::{DirectInstall, InPlaceUpgrade, PackageConfig, PackageManager};
use crate::tool::Spec;
use log::{info, warn};

pub enum Executor {
    Tool(Box<ToolCommand>),
    PackageInstall(Box<PackageInstallCommand>),
    PackageLink(Box<PackageLinkCommand>),
    PackageUpgrade(Box<PackageUpgradeCommand>),
    InternalInstall(Box<InternalInstallCommand>),
    Uninstall(Box<UninstallCommand>),
    Multiple(Vec<Executor>),
}

impl Executor {
    pub fn envs<K, V, S>(&mut self, envs: &HashMap<K, V, S>)
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        match self {
            Executor::Tool(cmd) => cmd.envs(envs),
            Executor::PackageInstall(cmd) => cmd.envs(envs),
            Executor::PackageLink(cmd) => cmd.envs(envs),
            Executor::PackageUpgrade(cmd) => cmd.envs(envs),
            // Internal installs use Volta's logic and don't rely on the environment variables
            Executor::InternalInstall(_) => {}
            // Uninstalls use Volta's logic and don't rely on environment variables
            Executor::Uninstall(_) => {}
            Executor::Multiple(executors) => {
                for exe in executors {
                    exe.envs(envs);
                }
            }
        }
    }

    pub fn cli_platform(&mut self, cli: CliPlatform) {
        match self {
            Executor::Tool(cmd) => cmd.cli_platform(cli),
            Executor::PackageInstall(cmd) => cmd.cli_platform(cli),
            Executor::PackageLink(cmd) => cmd.cli_platform(cli),
            Executor::PackageUpgrade(cmd) => cmd.cli_platform(cli),
            // Internal installs use Volta's logic and don't rely on the Node platform
            Executor::InternalInstall(_) => {}
            // Uninstall use Volta's logic and don't rely on the Node platform
            Executor::Uninstall(_) => {}
            Executor::Multiple(executors) => {
                for exe in executors {
                    exe.cli_platform(cli.clone());
                }
            }
        }
    }

    pub fn execute(self, session: &mut Session) -> Fallible<ExitStatus> {
        match self {
            Executor::Tool(cmd) => cmd.execute(session),
            Executor::PackageInstall(cmd) => cmd.execute(session),
            Executor::PackageLink(cmd) => cmd.execute(session),
            Executor::PackageUpgrade(cmd) => cmd.execute(session),
            Executor::InternalInstall(cmd) => cmd.execute(session),
            Executor::Uninstall(cmd) => cmd.execute(session),
            Executor::Multiple(executors) => {
                info!(
                    "{} Volta is processing each package separately",
                    note_prefix()
                );
                for exe in executors {
                    let status = exe.execute(session)?;
                    // If any of the sub-commands fail, then we should stop installing and return
                    // that failure.
                    if !status.success() {
                        return Ok(status);
                    }
                }
                // If we get here, then all of the sub-commands succeeded, so we should report success
                Ok(ExitStatus::from_raw(0))
            }
        }
    }
}

impl From<Vec<Executor>> for Executor {
    fn from(mut executors: Vec<Executor>) -> Self {
        if executors.len() == 1 {
            executors.pop().unwrap()
        } else {
            Executor::Multiple(executors)
        }
    }
}

/// Process builder for launching a Volta-managed tool
///
/// Tracks the Platform as well as what kind of tool is being executed, to allow individual tools
/// to customize the behavior before execution.
pub struct ToolCommand {
    command: Command,
    platform: Option<Platform>,
    kind: ToolKind,
}

/// The kind of tool being executed, used to determine the correct execution context
pub enum ToolKind {
    Node,
    Npm,
    Npx,
    Pnpm,
    Yarn,
    ProjectLocalBinary(String),
    DefaultBinary(String),
    Bypass(String),
}

impl ToolCommand {
    pub fn new<E, A, S>(exe: E, args: A, platform: Option<Platform>, kind: ToolKind) -> Self
    where
        E: AsRef<OsStr>,
        A: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = create_command(exe);
        command.args(args);

        Self {
            command,
            platform,
            kind,
        }
    }

    /// Adds or updates environment variables that the command will use
    pub fn envs<E, K, V>(&mut self, envs: E)
    where
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.envs(envs);
    }

    /// Adds or updates a single environment variable that the command will use
    pub fn env<K, V>(&mut self, key: K, value: V)
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.env(key, value);
    }

    /// Updates the Platform for the command to include values from the command-line
    pub fn cli_platform(&mut self, cli: CliPlatform) {
        self.platform = match self.platform.take() {
            Some(base) => Some(cli.merge(base)),
            None => cli.into(),
        };
    }

    /// Runs the command, returning the `ExitStatus` if it successfully launches
    pub fn execute(mut self, session: &mut Session) -> Fallible<ExitStatus> {
        let (path, on_failure) = match self.kind {
            ToolKind::Node => super::node::execution_context(self.platform, session)?,
            ToolKind::Npm => super::npm::execution_context(self.platform, session)?,
            ToolKind::Npx => super::npx::execution_context(self.platform, session)?,
            ToolKind::Pnpm => super::pnpm::execution_context(self.platform, session)?,
            ToolKind::Yarn => super::yarn::execution_context(self.platform, session)?,
            ToolKind::DefaultBinary(bin) => {
                super::binary::default_execution_context(bin, self.platform, session)?
            }
            ToolKind::ProjectLocalBinary(bin) => {
                super::binary::local_execution_context(bin, self.platform, session)?
            }
            ToolKind::Bypass(command) => (System::path()?, ErrorKind::BypassError { command }),
        };

        self.command.env(RECURSION_ENV_VAR, "1");
        self.command.env("PATH", path);

        pass_control_to_shim();
        self.command.status().with_context(|| on_failure)
    }
}

impl From<ToolCommand> for Executor {
    fn from(cmd: ToolCommand) -> Self {
        Executor::Tool(Box::new(cmd))
    }
}

/// Process builder for launching a package install command (e.g. `npm install --global`)
///
/// This will use a `DirectInstall` instance to modify the command before running to point it to
/// the Volta directory. It will also complete the install, writing config files and shims
pub struct PackageInstallCommand {
    /// The command that will ultimately be executed
    command: Command,
    /// The installer that modifies the command as necessary and provides the completion method
    installer: DirectInstall,
    /// The platform to use when running the command.
    platform: Platform,
}

impl PackageInstallCommand {
    pub fn new<A, S>(args: A, platform: Platform, manager: PackageManager) -> Fallible<Self>
    where
        A: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let installer = DirectInstall::new(manager)?;

        let mut command = match manager {
            PackageManager::Npm => create_command("npm"),
            PackageManager::Pnpm => create_command("pnpm"),
            PackageManager::Yarn => create_command("yarn"),
        };
        command.args(args);

        Ok(PackageInstallCommand {
            command,
            installer,
            platform,
        })
    }

    pub fn for_npm_link<A, S>(args: A, platform: Platform, name: String) -> Fallible<Self>
    where
        A: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let installer = DirectInstall::with_name(PackageManager::Npm, name)?;

        let mut command = create_command("npm");
        command.args(args);

        Ok(PackageInstallCommand {
            command,
            installer,
            platform,
        })
    }

    /// Adds or updates environment variables that the command will use
    pub fn envs<E, K, V>(&mut self, envs: E)
    where
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.envs(envs);
    }

    /// Updates the Platform for the command to include values from the command-line
    pub fn cli_platform(&mut self, cli: CliPlatform) {
        self.platform = cli.merge(self.platform.clone());
    }

    /// Runs the install command, applying the necessary modifications to install into the Volta
    /// data directory
    pub fn execute(mut self, session: &mut Session) -> Fallible<ExitStatus> {
        let _lock = VoltaLock::acquire();
        let image = self.platform.checkout(session)?;
        let path = image.path()?;

        self.command.env(RECURSION_ENV_VAR, "1");
        self.command.env("PATH", path);
        self.installer.setup_command(&mut self.command);

        let status = self
            .command
            .status()
            .with_context(|| ErrorKind::BinaryExecError)?;

        if status.success() {
            self.installer.complete_install(&image)?;
        }

        Ok(status)
    }
}

impl From<PackageInstallCommand> for Executor {
    fn from(cmd: PackageInstallCommand) -> Self {
        Executor::PackageInstall(Box::new(cmd))
    }
}

/// Process builder for launching a `npm link <package>` command
///
/// This will set the appropriate environment variables to ensure that the linked package can be
/// found.
pub struct PackageLinkCommand {
    /// The command that will ultimately be executed
    command: Command,
    /// The tool the user wants to link
    tool: String,
    /// The platform to use when running the command
    platform: Platform,
}

impl PackageLinkCommand {
    pub fn new<A, S>(args: A, platform: Platform, tool: String) -> Self
    where
        A: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = create_command("npm");
        command.args(args);

        PackageLinkCommand {
            command,
            tool,
            platform,
        }
    }

    /// Adds or updates environment variables that the command will use
    pub fn envs<E, K, V>(&mut self, envs: E)
    where
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.envs(envs);
    }

    /// Updates the Platform for the command to include values from the command-line
    pub fn cli_platform(&mut self, cli: CliPlatform) {
        self.platform = cli.merge(self.platform.clone());
    }

    /// Runs the link command, applying the necessary modifications to pull from the Volta data
    /// directory.
    ///
    /// This will also check for some common failure cases and alert the user
    pub fn execute(mut self, session: &mut Session) -> Fallible<ExitStatus> {
        self.check_linked_package(session)?;

        let image = self.platform.checkout(session)?;
        let path = image.path()?;

        self.command.env(RECURSION_ENV_VAR, "1");
        self.command.env("PATH", path);
        let package_root = volta_home()?.package_image_dir(&self.tool);
        PackageManager::Npm.setup_global_command(&mut self.command, package_root);

        self.command
            .status()
            .with_context(|| ErrorKind::BinaryExecError)
    }

    /// Check for possible failure cases with the linked package:
    ///     - The package is not found as a global
    ///     - The package exists, but was linked using a different package manager
    ///     - The package is using a different version of Node than the current project (warning)
    fn check_linked_package(&self, session: &mut Session) -> Fallible<()> {
        let config =
            PackageConfig::from_file(volta_home()?.default_package_config_file(&self.tool))
                .with_context(|| ErrorKind::NpmLinkMissingPackage {
                    package: self.tool.clone(),
                })?;

        if config.manager != PackageManager::Npm {
            return Err(ErrorKind::NpmLinkWrongManager {
                package: self.tool.clone(),
            }
            .into());
        }

        if let Some(platform) = session.project_platform()? {
            if platform.node.major != config.platform.node.major {
                warn!(
                    "the current project is using {}, but package '{}' was linked using {}. These might not interact correctly.",
                    tool_version("node", &platform.node),
                    self.tool,
                    tool_version("node", &config.platform.node)
                );
            }
        }

        Ok(())
    }
}

impl From<PackageLinkCommand> for Executor {
    fn from(cmd: PackageLinkCommand) -> Self {
        Executor::PackageLink(Box::new(cmd))
    }
}

/// Process builder for launching a global package upgrade command (e.g. `npm update -g`)
///
/// This will use an `InPlaceUpgrade` instance to modify the command and point at the appropriate
/// image directory. It will also complete the install, writing any updated configs and shims
pub struct PackageUpgradeCommand {
    /// The command that will ultimately be executed
    command: Command,
    /// Helper utility to modify the command and provide the completion method
    upgrader: InPlaceUpgrade,
    /// The platform to run the command under
    platform: Platform,
}

impl PackageUpgradeCommand {
    pub fn new<A, S>(
        args: A,
        package: String,
        platform: Platform,
        manager: PackageManager,
    ) -> Fallible<Self>
    where
        A: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let upgrader = InPlaceUpgrade::new(package, manager)?;

        let mut command = match manager {
            PackageManager::Npm => create_command("npm"),
            PackageManager::Pnpm => create_command("pnpm"),
            PackageManager::Yarn => create_command("yarn"),
        };
        command.args(args);

        Ok(PackageUpgradeCommand {
            command,
            upgrader,
            platform,
        })
    }

    /// Adds or updates environment variables that the command will use
    pub fn envs<E, K, V>(&mut self, envs: E)
    where
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.envs(envs);
    }

    /// Updates the Platform for the command to include values from the command-line
    pub fn cli_platform(&mut self, cli: CliPlatform) {
        self.platform = cli.merge(self.platform.clone());
    }

    /// Runs the upgrade command, applying the necessary modifications to point at the Volta image
    /// directory
    ///
    /// Will also check for common failure cases, such as non-existant package or wrong package
    /// manager
    pub fn execute(mut self, session: &mut Session) -> Fallible<ExitStatus> {
        self.upgrader.check_upgraded_package()?;

        let _lock = VoltaLock::acquire();
        let image = self.platform.checkout(session)?;
        let path = image.path()?;

        self.command.env(RECURSION_ENV_VAR, "1");
        self.command.env("PATH", path);
        self.upgrader.setup_command(&mut self.command);

        let status = self
            .command
            .status()
            .with_context(|| ErrorKind::BinaryExecError)?;

        if status.success() {
            self.upgrader.complete_upgrade(&image)?;
        }

        Ok(status)
    }
}

impl From<PackageUpgradeCommand> for Executor {
    fn from(cmd: PackageUpgradeCommand) -> Self {
        Executor::PackageUpgrade(Box::new(cmd))
    }
}

/// Executor for running an internal install (installing Node, npm, pnpm or Yarn using the `volta
/// install` logic)
///
/// Note: This is not intended to be used for Package installs. Those should go through the
/// `PackageInstallCommand` above, to more seamlessly integrate with the package manager
pub struct InternalInstallCommand {
    tool: Spec,
}

impl InternalInstallCommand {
    pub fn new(tool: Spec) -> Self {
        InternalInstallCommand { tool }
    }

    /// Runs the install, using Volta's internal install logic for the appropriate tool
    fn execute(self, session: &mut Session) -> Fallible<ExitStatus> {
        info!(
            "{} using Volta to install {}",
            note_prefix(),
            self.tool.name()
        );

        self.tool.resolve(session)?.install(session)?;

        Ok(ExitStatus::from_raw(0))
    }
}

impl From<InternalInstallCommand> for Executor {
    fn from(cmd: InternalInstallCommand) -> Self {
        Executor::InternalInstall(Box::new(cmd))
    }
}

/// Executor for running a tool uninstall command.
///
/// This will use the `volta uninstall` logic to correctly ensure that the package is fully
/// uninstalled
pub struct UninstallCommand {
    tool: Spec,
}

impl UninstallCommand {
    pub fn new(tool: Spec) -> Self {
        UninstallCommand { tool }
    }

    /// Runs the uninstall with Volta's internal uninstall logic
    fn execute(self, session: &mut Session) -> Fallible<ExitStatus> {
        info!(
            "{} using Volta to uninstall {}",
            note_prefix(),
            self.tool.name()
        );

        self.tool.resolve(session)?.uninstall(session)?;

        Ok(ExitStatus::from_raw(0))
    }
}

impl From<UninstallCommand> for Executor {
    fn from(cmd: UninstallCommand) -> Self {
        Executor::Uninstall(Box::new(cmd))
    }
}
