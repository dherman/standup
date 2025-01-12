use std::env;
use std::ffi::OsStr;
use std::iter::once;

use super::executor::{
    Executor, InternalInstallCommand, PackageInstallCommand, PackageLinkCommand,
    PackageUpgradeCommand, UninstallCommand,
};
use crate::error::{ErrorKind, Fallible};
use crate::inventory::package_configs;
use crate::platform::{Platform, PlatformSpec};
use crate::tool::package::PackageManager;
use crate::tool::Spec;
use log::debug;

const UNSAFE_GLOBAL: &str = "VOLTA_UNSAFE_GLOBAL";
/// Aliases that npm supports for the 'install' command
const NPM_INSTALL_ALIASES: [&str; 12] = [
    "i", "in", "ins", "inst", "insta", "instal", "install", "isnt", "isnta", "isntal", "isntall",
    "add",
];
/// Aliases that npm supports for the 'uninstall' command
const NPM_UNINSTALL_ALIASES: [&str; 5] = ["un", "uninstall", "remove", "rm", "r"];
/// Aliases that npm supports for the 'link' command
const NPM_LINK_ALIASES: [&str; 2] = ["link", "ln"];
/// Aliases that npm supports for the `update` command
const NPM_UPDATE_ALIASES: [&str; 4] = ["update", "udpate", "upgrade", "up"];
/// Aliases that pnpm supports for the 'remove' command,
/// see: https://pnpm.io/cli/remove
const PNPM_UNINSTALL_ALIASES: [&str; 4] = ["remove", "uninstall", "rm", "un"];
/// Aliases that pnpm supports for the 'update' command,
/// see: https://pnpm.io/cli/update
const PNPM_UPDATE_ALIASES: [&str; 3] = ["update", "upgrade", "up"];
/// Aliases that pnpm supports for the 'link' command
/// see: https://pnpm.io/cli/link
const PNPM_LINK_ALIASES: [&str; 2] = ["link", "ln"];

pub enum CommandArg<'a> {
    Global(GlobalCommand<'a>),
    Intercepted(InterceptedCommand<'a>),
    Standard,
}

impl<'a> CommandArg<'a> {
    /// Parse the given set of arguments to see if they correspond to an intercepted npm command
    pub fn for_npm<S>(args: &'a [S]) -> Self
    where
        S: AsRef<OsStr>,
    {
        // If VOLTA_UNSAFE_GLOBAL is set, then we always skip any interception parsing
        if env::var_os(UNSAFE_GLOBAL).is_some() {
            return CommandArg::Standard;
        }

        let mut positionals = args.iter().filter(is_positional).map(AsRef::as_ref);

        // The first positional argument will always be the command, however npm supports multiple
        // aliases for commands (see https://github.com/npm/cli/blob/latest/lib/utils/cmd-list.js)
        // Additionally, if we have a global install or uninstall, all of the remaining positional
        // arguments will be the tools to install or uninstall. If there are _no_ other arguments,
        // then we treat the command not a global and allow npm to handle any error messages.
        match positionals.next() {
            Some(cmd) if NPM_INSTALL_ALIASES.iter().any(|a| a == &cmd) => {
                if has_global_without_prefix(args) {
                    let tools: Vec<_> = positionals.collect();

                    if tools.is_empty() {
                        CommandArg::Standard
                    } else {
                        // The common args for an install should be the command combined with any flags
                        let mut common_args = vec![cmd];
                        common_args.extend(args.iter().filter(is_flag).map(AsRef::as_ref));

                        CommandArg::Global(GlobalCommand::Install(InstallArgs {
                            manager: PackageManager::Npm,
                            common_args,
                            tools,
                        }))
                    }
                } else {
                    CommandArg::Standard
                }
            }
            Some(cmd) if NPM_UNINSTALL_ALIASES.iter().any(|a| a == &cmd) => {
                if has_global_without_prefix(args) {
                    let tools: Vec<_> = positionals.collect();

                    if tools.is_empty() {
                        CommandArg::Standard
                    } else {
                        CommandArg::Global(GlobalCommand::Uninstall(UninstallArgs { tools }))
                    }
                } else {
                    CommandArg::Standard
                }
            }
            Some(cmd) if cmd == "unlink" => {
                let tools: Vec<_> = positionals.collect();

                if tools.is_empty() {
                    // `npm unlink` without any arguments is used to unlink the current project
                    CommandArg::Intercepted(InterceptedCommand::Unlink)
                } else if has_global_without_prefix(args) {
                    // With arguments, `npm unlink` is an alias of `npm remove`
                    CommandArg::Global(GlobalCommand::Uninstall(UninstallArgs { tools }))
                } else {
                    CommandArg::Standard
                }
            }
            Some(cmd) if NPM_LINK_ALIASES.iter().any(|a| a == &cmd) => {
                // Much like install, the common args for a link are the command combined with any flags
                let mut common_args = vec![cmd];
                common_args.extend(args.iter().filter(is_flag).map(AsRef::as_ref));
                let tools: Vec<_> = positionals.collect();

                CommandArg::Intercepted(InterceptedCommand::Link(LinkArgs { common_args, tools }))
            }
            Some(cmd) if NPM_UPDATE_ALIASES.iter().any(|a| a == &cmd) => {
                if has_global_without_prefix(args) {
                    // Once again, the common args are the command combined with any flags
                    let mut common_args = vec![cmd];
                    common_args.extend(args.iter().filter(is_flag).map(AsRef::as_ref));
                    let tools: Vec<_> = positionals.collect();

                    CommandArg::Global(GlobalCommand::Upgrade(UpgradeArgs {
                        common_args,
                        tools,
                        manager: PackageManager::Npm,
                    }))
                } else {
                    CommandArg::Standard
                }
            }
            _ => CommandArg::Standard,
        }
    }

    /// Parse the given set of arguments to see if they correspond to an intercepted pnpm command
    #[allow(dead_code)]
    pub fn for_pnpm<S>(args: &'a [S]) -> CommandArg<'a>
    where
        S: AsRef<OsStr>,
    {
        // If VOLTA_UNSAFE_GLOBAL is set, then we always skip any global parsing
        if env::var_os(UNSAFE_GLOBAL).is_some() {
            return CommandArg::Standard;
        }

        let (flags, positionals): (Vec<&OsStr>, Vec<&OsStr>) =
            args.iter().map(AsRef::<OsStr>::as_ref).partition(is_flag);

        // The first positional argument will always be the subcommand for pnpm
        match positionals.split_first() {
            None => CommandArg::Standard,
            Some((&subcommand, tools)) => {
                let is_global = flags.iter().any(|&f| f == "--global" || f == "-g");
                // Do not intercept if a custom global dir is explicitly specified
                // See: https://pnpm.io/npmrc#global-dir
                let prefixed = flags.iter().any(|&f| f == "--global-dir");

                // pnpm subcommands that support the `global` flag:
                // `add`, `update`, `remove`, `link`, `list`, `outdated`,
                // `why`, `env`, `root`, `bin`.
                match is_global && !prefixed {
                    false => CommandArg::Standard,
                    true => match subcommand.to_str() {
                        // `add`
                        Some("add") => {
                            let manager = PackageManager::Pnpm;
                            let mut common_args = vec![subcommand];
                            common_args.extend(flags);

                            CommandArg::Global(GlobalCommand::Install(InstallArgs {
                                manager,
                                common_args,
                                tools: tools.to_vec(),
                            }))
                        }
                        // `update`
                        Some(cmd) if PNPM_UPDATE_ALIASES.iter().any(|&a| a == cmd) => {
                            let manager = PackageManager::Pnpm;
                            let mut common_args = vec![subcommand];
                            common_args.extend(flags);
                            CommandArg::Global(GlobalCommand::Upgrade(UpgradeArgs {
                                manager,
                                common_args,
                                tools: tools.to_vec(),
                            }))
                        }
                        // `remove`
                        Some(cmd) if PNPM_UNINSTALL_ALIASES.iter().any(|&a| a == cmd) => {
                            CommandArg::Global(GlobalCommand::Uninstall(UninstallArgs {
                                tools: tools.to_vec(),
                            }))
                        }
                        // `link`
                        Some(cmd) if PNPM_LINK_ALIASES.iter().any(|&a| a == cmd) => {
                            let mut common_args = vec![subcommand];
                            common_args.extend(flags);
                            CommandArg::Intercepted(InterceptedCommand::Link(LinkArgs {
                                common_args,
                                tools: tools.to_vec(),
                            }))
                        }
                        _ => CommandArg::Standard,
                    },
                }
            }
        }
    }

    /// Parse the given set of arguments to see if they correspond to an intercepted Yarn command
    pub fn for_yarn<S>(args: &'a [S]) -> Self
    where
        S: AsRef<OsStr>,
    {
        // If VOLTA_UNSAFE_GLOBAL is set, then we always skip any global parsing
        if env::var_os(UNSAFE_GLOBAL).is_some() {
            return CommandArg::Standard;
        }

        let mut positionals = args.iter().filter(is_positional).map(AsRef::as_ref);

        // Yarn globals must always start with `global <command>`
        // If we have a global add, remove, or upgrade, then all of the remaining positional
        // arguments will be the tools to modify. As with npm, if there are no arguments then we
        // can treat it as if it's not a global command and allow Yarn to show any errors.
        match (positionals.next(), positionals.next()) {
            (Some(global), Some(add)) if global == "global" && add == "add" => {
                let tools: Vec<_> = positionals.collect();

                if tools.is_empty() {
                    CommandArg::Standard
                } else {
                    // The common args for an install should be `global add` and any flags
                    let mut common_args = vec![global, add];
                    common_args.extend(args.iter().filter(is_flag).map(AsRef::as_ref));

                    CommandArg::Global(GlobalCommand::Install(InstallArgs {
                        manager: PackageManager::Yarn,
                        common_args,
                        tools,
                    }))
                }
            }
            (Some(global), Some(remove)) if global == "global" && remove == "remove" => {
                let tools: Vec<_> = positionals.collect();

                if tools.is_empty() {
                    CommandArg::Standard
                } else {
                    CommandArg::Global(GlobalCommand::Uninstall(UninstallArgs { tools }))
                }
            }
            (Some(global), Some(upgrade)) if global == "global" && upgrade == "upgrade" => {
                // The common args for an upgrade should be `global upgrade` and any flags
                let mut common_args = vec![global, upgrade];
                common_args.extend(args.iter().filter(is_flag).map(AsRef::as_ref));

                CommandArg::Global(GlobalCommand::Upgrade(UpgradeArgs {
                    common_args,
                    tools: positionals.collect(),
                    manager: PackageManager::Yarn,
                }))
            }
            _ => CommandArg::Standard,
        }
    }
}

pub enum GlobalCommand<'a> {
    Install(InstallArgs<'a>),
    Uninstall(UninstallArgs<'a>),
    Upgrade(UpgradeArgs<'a>),
}

impl GlobalCommand<'_> {
    pub fn executor(self, platform: &PlatformSpec) -> Fallible<Executor> {
        match self {
            GlobalCommand::Install(cmd) => cmd.executor(platform),
            GlobalCommand::Uninstall(cmd) => cmd.executor(),
            GlobalCommand::Upgrade(cmd) => cmd.executor(platform),
        }
    }
}

/// The arguments passed to a global install command
pub struct InstallArgs<'a> {
    /// The package manager being used
    manager: PackageManager,
    /// Common arguments that apply to each tool (e.g. flags)
    common_args: Vec<&'a OsStr>,
    /// The individual tool arguments
    tools: Vec<&'a OsStr>,
}

impl InstallArgs<'_> {
    /// Convert these global install arguments into an executor for the command
    ///
    /// If there are multiple packages specified to install, then they will be broken out into
    /// individual commands and run separately. That allows us to keep Volta's sandboxing for each
    /// package while still supporting the ability to install multiple packages at once.
    pub fn executor(self, platform_spec: &PlatformSpec) -> Fallible<Executor> {
        let mut executors = Vec::with_capacity(self.tools.len());

        for tool in self.tools {
            // External tool installs may be in a form that doesn't match a `Spec` (such as a git
            // URL or path to a tarball). If parsing into a `Spec` fails, we assume that it's a
            // 3rd-party Tool and attempt to install anyway.
            match Spec::try_from_str(&tool.to_string_lossy()) {
                Ok(Spec::Package(_, _)) | Err(_) => {
                    let platform = platform_spec.as_default();
                    // The args for an individual install command are the common args combined
                    // with the name of the tool.
                    let args = self.common_args.iter().chain(once(&tool));
                    let command = PackageInstallCommand::new(args, platform, self.manager)?;
                    executors.push(command.into());
                }
                Ok(internal) => {
                    // TODO: gotta fail here or something?
                    executors.push(InternalInstallCommand::new(internal).into())
                }
            }
        }

        Ok(executors.into())
    }
}

/// The list of tools passed to an uninstall command
pub struct UninstallArgs<'a> {
    tools: Vec<&'a OsStr>,
}

impl UninstallArgs<'_> {
    /// Convert the tools into an executor for the uninstall command
    ///
    /// Since the packages are sandboxed, each needs to be uninstalled separately
    pub fn executor(self) -> Fallible<Executor> {
        let mut executors = Vec::with_capacity(self.tools.len());

        for tool_name in self.tools {
            let tool = Spec::try_from_str(&tool_name.to_string_lossy())?;
            executors.push(UninstallCommand::new(tool).into());
        }

        Ok(executors.into())
    }
}

/// The list of tools passed to an upgrade command
pub struct UpgradeArgs<'a> {
    /// The package manager being used
    manager: PackageManager,
    /// Common arguments that apply to each tool (e.g. flags)
    common_args: Vec<&'a OsStr>,
    /// The individual tool arguments
    tools: Vec<&'a OsStr>,
}

impl UpgradeArgs<'_> {
    /// Convert these global upgrade arguments into an executor for the command
    ///
    /// If there are multiple packages specified to upgrade, then they will be broken out into
    /// individual commands and run separately. If no packages are specified, then we will upgrade
    /// _all_ installed packages that were installed with the same package manager.
    pub fn executor(self, platform_spec: &PlatformSpec) -> Fallible<Executor> {
        if self.tools.is_empty() {
            return self.executor_all_packages(platform_spec);
        }

        let mut executors = Vec::with_capacity(self.tools.len());

        for tool in self.tools {
            match Spec::try_from_str(&tool.to_string_lossy()) {
                Ok(Spec::Package(package, _)) => {
                    let platform = platform_spec.as_default();
                    let args = self.common_args.iter().chain(once(&tool));
                    executors.push(
                        PackageUpgradeCommand::new(args, package, platform, self.manager)?.into(),
                    );
                }
                Ok(internal) => {
                    executors.push(UninstallCommand::new(internal).into());
                }
                Err(_) => {
                    return Err(ErrorKind::UpgradePackageNotFound {
                        package: tool.to_string_lossy().to_string(),
                        manager: self.manager,
                    }
                    .into())
                }
            }
        }

        Ok(executors.into())
    }

    /// Build an executor to upgrade _all_ global packages that were installed with the same
    /// package manager as we are currently running.
    fn executor_all_packages(self, platform_spec: &PlatformSpec) -> Fallible<Executor> {
        package_configs()?
            .into_iter()
            .filter(|config| config.manager == self.manager)
            .map(|config| {
                let platform = platform_spec.as_default();
                let package_name = config.name.as_ref();
                let args = self.common_args.iter().chain(once(&package_name));

                let executor =
                    PackageUpgradeCommand::new(args, config.name.clone(), platform, self.manager)?
                        .into();
                Ok(executor)
            })
            .collect::<Fallible<Vec<_>>>()
            .map(Into::into)
    }
}

/// An intercepted local command
pub enum InterceptedCommand<'a> {
    Link(LinkArgs<'a>),
    Unlink,
}

/// The arguments passed to an `npm link` command
pub struct LinkArgs<'a> {
    /// The common arguments that apply to each tool
    common_args: Vec<&'a OsStr>,
    /// The list of tools to link (if any)
    tools: Vec<&'a OsStr>,
}

impl LinkArgs<'_> {
    pub fn executor(self, platform: Platform, project_name: Option<String>) -> Fallible<Executor> {
        if self.tools.is_empty() {
            // If no tools are specified, then this is a bare link command, linking the current
            // project as a global package. We treat this like a global install except we look up
            // the name from the current directory first.
            match project_name {
                Some(name) => PackageInstallCommand::for_npm_link(self.common_args, platform, name),
                None => PackageInstallCommand::new(self.common_args, platform, PackageManager::Npm),
            }
            .map(Into::into)
        } else {
            // If there are tools specified, then this represents a command to link a global
            // package into the current project. We handle each tool separately to support Volta's
            // package sandboxing.
            let common_args = self.common_args;

            Ok(self
                .tools
                .into_iter()
                .map(|tool| {
                    let args = common_args.iter().chain(once(&tool));
                    PackageLinkCommand::new(
                        args,
                        platform.clone(),
                        tool.to_string_lossy().to_string(),
                    )
                    .into()
                })
                .collect::<Vec<_>>()
                .into())
        }
    }
}

/// Check if the provided argument list includes a global flag and _doesn't_ have a prefix setting
///
/// For our interception, we only want to intercept global commands. Additionally, if the user
/// passes a prefix setting, that will override the logic we use to redirect the install, so our
/// process won't work and will cause an error. We should avoid intercepting in those cases since
/// a command with an explicit prefix is something beyond the "standard" global install anyway.
fn has_global_without_prefix<A>(args: &[A]) -> bool
where
    A: AsRef<OsStr>,
{
    let (has_global, has_prefix) = args.iter().fold((false, false), |(global, prefix), arg| {
        match arg.as_ref().to_str() {
            Some("-g") | Some("--global") => (true, prefix),
            Some(pre) if pre.starts_with("--prefix") => (global, true),
            _ => (global, prefix),
        }
    });

    if has_global && has_prefix {
        debug!("Skipping global interception due to prefix argument");
    }

    has_global && !has_prefix
}

fn is_flag<A>(arg: &A) -> bool
where
    A: AsRef<OsStr>,
{
    match arg.as_ref().to_str() {
        Some(a) => a.starts_with('-'),
        None => false,
    }
}

fn is_positional<A>(arg: &A) -> bool
where
    A: AsRef<OsStr>,
{
    !is_flag(arg)
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};

    fn arg_list<A, S>(args: A) -> Vec<OsString>
    where
        A: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        args.into_iter().map(|a| a.as_ref().to_owned()).collect()
    }

    mod npm {
        use super::super::*;
        use super::arg_list;

        #[test]
        fn handles_global_install() {
            match CommandArg::for_npm(&arg_list(["install", "--global", "typescript@3"])) {
                CommandArg::Global(GlobalCommand::Install(install)) => {
                    assert_eq!(install.manager, PackageManager::Npm);
                    assert_eq!(install.common_args, vec!["install", "--global"]);
                    assert_eq!(install.tools, vec!["typescript@3"]);
                }
                _ => panic!("Doesn't parse global install as a global"),
            };
        }

        #[test]
        fn handles_local_install() {
            match CommandArg::for_npm(&arg_list(["install", "--save-dev", "typescript"])) {
                CommandArg::Standard => (),
                _ => panic!("Parses local install as global"),
            };
        }

        #[test]
        fn handles_global_uninstall() {
            match CommandArg::for_npm(&arg_list(["uninstall", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Uninstall(uninstall)) => {
                    assert_eq!(uninstall.tools, vec!["typescript"]);
                }
                _ => panic!("Doesn't parse global uninstall as a global"),
            };
        }

        #[test]
        fn handles_local_uninstall() {
            match CommandArg::for_npm(&arg_list(["uninstall", "--save-dev", "typescript"])) {
                CommandArg::Standard => (),
                _ => panic!("Parses local uninstall as global"),
            };
        }

        #[test]
        fn handles_multiple_install() {
            match CommandArg::for_npm(&arg_list([
                "install",
                "--global",
                "typescript@3",
                "cowsay@1",
                "ember-cli@2",
            ])) {
                CommandArg::Global(GlobalCommand::Install(install)) => {
                    assert_eq!(install.manager, PackageManager::Npm);
                    assert_eq!(install.common_args, vec!["install", "--global"]);
                    assert_eq!(
                        install.tools,
                        vec!["typescript@3", "cowsay@1", "ember-cli@2"]
                    );
                }
                _ => panic!("Doesn't parse global install as a global"),
            };
        }

        #[test]
        fn handles_multiple_uninstall() {
            match CommandArg::for_npm(&arg_list([
                "uninstall",
                "--global",
                "typescript",
                "cowsay",
                "ember-cli",
            ])) {
                CommandArg::Global(GlobalCommand::Uninstall(uninstall)) => {
                    assert_eq!(uninstall.tools, vec!["typescript", "cowsay", "ember-cli"]);
                }
                _ => panic!("Doesn't parse global uninstall as a global"),
            };
        }

        #[test]
        fn handles_bare_link() {
            match CommandArg::for_npm(&arg_list(["link"])) {
                CommandArg::Intercepted(InterceptedCommand::Link(_)) => (),
                _ => panic!("Doesn't parse bare link command ('npm link' with no packages"),
            };
        }

        #[test]
        fn handles_multiple_link() {
            match CommandArg::for_npm(&arg_list(["link", "typescript", "react"])) {
                CommandArg::Intercepted(InterceptedCommand::Link(link)) => {
                    assert_eq!(link.tools, vec!["typescript", "react"]);
                }
                _ => panic!("Doesn't parse link command with packages"),
            };
        }

        #[test]
        fn handles_bare_unlink() {
            match CommandArg::for_npm(&arg_list(["unlink"])) {
                CommandArg::Intercepted(InterceptedCommand::Unlink) => (),
                _ => panic!("Doesn't parse bare unlink command ('npm unlink' with no packages"),
            };
        }

        #[test]
        fn handles_local_unlink() {
            match CommandArg::for_npm(&arg_list(["unlink", "@angular/cli"])) {
                CommandArg::Standard => (),
                _ => panic!("Doesn't pass through local 'unlink' command"),
            }
        }

        #[test]
        fn handles_global_aliases() {
            match CommandArg::for_npm(&arg_list(["install", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse long form (--global)"),
            };

            match CommandArg::for_npm(&arg_list(["install", "-g", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse short form (-g)"),
            };
        }

        #[test]
        fn handles_install_aliases() {
            match CommandArg::for_npm(&arg_list(["i", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse short form (i)"),
            };

            match CommandArg::for_npm(&arg_list(["in", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse short form (in)"),
            };

            match CommandArg::for_npm(&arg_list(["ins", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse short form (ins)"),
            };

            match CommandArg::for_npm(&arg_list(["inst", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse short form (inst)"),
            };

            match CommandArg::for_npm(&arg_list(["insta", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse short form (insta)"),
            };

            match CommandArg::for_npm(&arg_list(["instal", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse short form (instal)"),
            };

            match CommandArg::for_npm(&arg_list(["install", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse exact command (install)"),
            };

            match CommandArg::for_npm(&arg_list(["isnt", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse short form misspelling (isnt)"),
            };

            match CommandArg::for_npm(&arg_list(["isnta", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse short form misspelling (isnta)"),
            };

            match CommandArg::for_npm(&arg_list(["isntal", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse short form misspelling (isntal)"),
            };

            match CommandArg::for_npm(&arg_list(["isntall", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse misspelling (isntall)"),
            };

            match CommandArg::for_npm(&arg_list(["add", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(_)) => (),
                _ => panic!("Doesn't parse 'add' alias"),
            };
        }

        #[test]
        fn handles_uninstall_aliases() {
            match CommandArg::for_npm(&arg_list(["uninstall", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Uninstall(_)) => (),
                _ => panic!("Doesn't parse long form (uninstall)"),
            };

            match CommandArg::for_npm(&arg_list(["unlink", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Uninstall(_)) => (),
                _ => panic!("Doesn't parse 'unlink'"),
            };

            match CommandArg::for_npm(&arg_list(["remove", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Uninstall(_)) => (),
                _ => panic!("Doesn't parse 'remove'"),
            };

            match CommandArg::for_npm(&arg_list(["un", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Uninstall(_)) => (),
                _ => panic!("Doesn't parse short form (un)"),
            };

            match CommandArg::for_npm(&arg_list(["rm", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Uninstall(_)) => (),
                _ => panic!("Doesn't parse short form (rm)"),
            };

            match CommandArg::for_npm(&arg_list(["r", "--global", "typescript"])) {
                CommandArg::Global(GlobalCommand::Uninstall(_)) => (),
                _ => panic!("Doesn't parse short form (r)"),
            };
        }

        #[test]
        fn handles_link_aliases() {
            match CommandArg::for_npm(&arg_list(["link"])) {
                CommandArg::Intercepted(InterceptedCommand::Link(_)) => (),
                _ => panic!("Doesn't parse long form (link)"),
            };

            match CommandArg::for_npm(&arg_list(["ln"])) {
                CommandArg::Intercepted(InterceptedCommand::Link(_)) => (),
                _ => panic!("Doesn't parse short form (ln)"),
            };
        }

        #[test]
        fn processes_flags() {
            match CommandArg::for_npm(&arg_list([
                "--global",
                "install",
                "typescript",
                "--no-audit",
                "cowsay",
                "--no-update-notifier",
            ])) {
                CommandArg::Global(GlobalCommand::Install(install)) => {
                    // The command gets moved to the front of common_args
                    assert_eq!(
                        install.common_args,
                        vec!["install", "--global", "--no-audit", "--no-update-notifier"]
                    );
                    assert_eq!(install.tools, vec!["typescript", "cowsay"]);
                }
                _ => panic!("Doesn't parse install with extra flags as a global"),
            };

            match CommandArg::for_npm(&arg_list([
                "uninstall",
                "--silent",
                "typescript",
                "-g",
                "cowsay",
            ])) {
                CommandArg::Global(GlobalCommand::Uninstall(uninstall)) => {
                    assert_eq!(uninstall.tools, vec!["typescript", "cowsay"]);
                }
                _ => panic!("Doesn't parse uninstall with extra flags as a global"),
            }
        }

        #[test]
        fn skips_commands_with_prefix() {
            match CommandArg::for_npm(&arg_list(["install", "-g", "--prefix", "~/", "ember"])) {
                CommandArg::Standard => {}
                _ => panic!("Parsed command with prefix as a global"),
            }

            match CommandArg::for_npm(&arg_list(["install", "-g", "--prefix=~/", "ember"])) {
                CommandArg::Standard => {}
                _ => panic!("Parsed command with prefix as a global"),
            }

            match CommandArg::for_npm(&arg_list(["uninstall", "-g", "--prefix", "~/", "ember"])) {
                CommandArg::Standard => {}
                _ => panic!("Parsed command with prefix as a global"),
            }

            match CommandArg::for_npm(&arg_list(["uninstall", "-g", "--prefix=~/", "ember"])) {
                CommandArg::Standard => {}
                _ => panic!("Parsed command with prefix as a global"),
            }

            match CommandArg::for_npm(&arg_list(["unlink", "-g", "--prefix", "~/", "ember"])) {
                CommandArg::Standard => {}
                _ => panic!("Parsed command with prefix as a global"),
            }

            match CommandArg::for_npm(&arg_list(["unlink", "-g", "--prefix=~/", "ember"])) {
                CommandArg::Standard => {}
                _ => panic!("Parsed command with prefix as a global"),
            }

            match CommandArg::for_npm(&arg_list(["update", "-g", "--prefix", "~/"])) {
                CommandArg::Standard => {}
                _ => panic!("Parsed command with prefix as a global"),
            }

            match CommandArg::for_npm(&arg_list(["update", "-g", "--prefix=~/"])) {
                CommandArg::Standard => {}
                _ => panic!("Parsed command with prefix as a global"),
            }
        }
    }

    mod yarn {
        use super::super::*;
        use super::*;

        #[test]
        fn handles_global_add() {
            match CommandArg::for_yarn(&arg_list(["global", "add", "typescript"])) {
                CommandArg::Global(GlobalCommand::Install(install)) => {
                    assert_eq!(install.manager, PackageManager::Yarn);
                    assert_eq!(install.common_args, vec!["global", "add"]);
                    assert_eq!(install.tools, vec!["typescript"]);
                }
                _ => panic!("Doesn't parse global add as a global"),
            };
        }

        #[test]
        fn handles_local_add() {
            match CommandArg::for_yarn(&arg_list(["add", "typescript"])) {
                CommandArg::Standard => (),
                _ => panic!("Parses local add as a global"),
            };

            match CommandArg::for_yarn(&arg_list(["add", "global"])) {
                CommandArg::Standard => (),
                _ => panic!("Incorrectly handles bad order"),
            };
        }

        #[test]
        fn handles_global_remove() {
            match CommandArg::for_yarn(&arg_list(["global", "remove", "typescript"])) {
                CommandArg::Global(GlobalCommand::Uninstall(uninstall)) => {
                    assert_eq!(uninstall.tools, vec!["typescript"]);
                }
                _ => panic!("Doesn't parse global remove as a global"),
            };
        }

        #[test]
        fn handles_local_remove() {
            match CommandArg::for_yarn(&arg_list(["remove", "typescript"])) {
                CommandArg::Standard => (),
                _ => panic!("Parses local remove as a global"),
            };

            match CommandArg::for_yarn(&arg_list(["remove", "global"])) {
                CommandArg::Standard => (),
                _ => panic!("Incorrectly handles bad order"),
            };
        }

        #[test]
        fn handles_multiple_add() {
            match CommandArg::for_yarn(&arg_list([
                "global",
                "add",
                "typescript",
                "cowsay",
                "ember-cli",
            ])) {
                CommandArg::Global(GlobalCommand::Install(install)) => {
                    assert_eq!(install.manager, PackageManager::Yarn);
                    assert_eq!(install.common_args, vec!["global", "add"]);
                    assert_eq!(install.tools, vec!["typescript", "cowsay", "ember-cli"]);
                }
                _ => panic!("Doesn't parse global add as a global"),
            };
        }

        #[test]
        fn handles_multiple_remove() {
            match CommandArg::for_yarn(&arg_list([
                "global",
                "remove",
                "typescript",
                "cowsay",
                "ember-cli",
            ])) {
                CommandArg::Global(GlobalCommand::Uninstall(uninstall)) => {
                    assert_eq!(uninstall.tools, vec!["typescript", "cowsay", "ember-cli"]);
                }
                _ => panic!("Doesn't parse global remove as a global"),
            };
        }

        #[test]
        fn processes_flags() {
            match CommandArg::for_yarn(&arg_list([
                "global",
                "--silent",
                "add",
                "ember-cli",
                "--prefix=~/",
                "typescript",
            ])) {
                CommandArg::Global(GlobalCommand::Install(install)) => {
                    // The commands get moved to the front of common_args
                    assert_eq!(
                        install.common_args,
                        vec!["global", "add", "--silent", "--prefix=~/"]
                    );
                    assert_eq!(install.tools, vec!["ember-cli", "typescript"]);
                }
                _ => panic!("Doesn't parse global add as a global"),
            };

            match CommandArg::for_yarn(&arg_list([
                "global",
                "--silent",
                "remove",
                "ember-cli",
                "--prefix=~/",
                "typescript",
            ])) {
                CommandArg::Global(GlobalCommand::Uninstall(uninstall)) => {
                    assert_eq!(uninstall.tools, vec!["ember-cli", "typescript"]);
                }
                _ => panic!("Doesn't parse global add as a global"),
            };
        }
    }
}
