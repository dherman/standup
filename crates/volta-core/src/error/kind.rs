use std::fmt;
use std::path::PathBuf;

use super::ExitCode;
use crate::style::{text_width, tool_version};
use crate::tool;
use crate::tool::package::PackageManager;
use textwrap::{fill, indent};

const REPORT_BUG_CTA: &str =
    "Please rerun the command that triggered this error with the environment
variable `VOLTA_LOGLEVEL` set to `debug` and open an issue at
https://github.com/volta-cli/volta/issues with the details!";

const PERMISSIONS_CTA: &str = "Please ensure you have correct permissions to the Volta directory.";

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum ErrorKind {
    /// Thrown when package tries to install a binary that is already installed.
    BinaryAlreadyInstalled {
        bin_name: String,
        existing_package: String,
        new_package: String,
    },

    /// Thrown when executing an external binary fails
    BinaryExecError,

    /// Thrown when a binary could not be found in the local inventory
    BinaryNotFound {
        name: String,
    },

    /// Thrown when building the virtual environment path fails
    BuildPathError,

    /// Thrown when unable to launch a command with VOLTA_BYPASS set
    BypassError {
        command: String,
    },

    /// Thrown when a user tries to `volta fetch` something other than node/yarn/npm.
    CannotFetchPackage {
        package: String,
    },

    /// Thrown when a user tries to `volta pin` something other than node/yarn/npm.
    CannotPinPackage {
        package: String,
    },

    /// Thrown when the Completions out-dir is not a directory
    CompletionsOutFileError {
        path: PathBuf,
    },

    /// Thrown when the containing directory could not be determined
    ContainingDirError {
        path: PathBuf,
    },

    CouldNotDetermineTool,

    /// Thrown when unable to start the migration executable
    CouldNotStartMigration,

    CreateDirError {
        dir: PathBuf,
    },

    /// Thrown when unable to create the layout file
    CreateLayoutFileError {
        file: PathBuf,
    },

    /// Thrown when unable to create a link to the shared global library directory
    CreateSharedLinkError {
        name: String,
    },

    /// Thrown when creating a temporary directory fails
    CreateTempDirError {
        in_dir: PathBuf,
    },

    /// Thrown when creating a temporary file fails
    CreateTempFileError {
        in_dir: PathBuf,
    },

    CurrentDirError,

    /// Thrown when deleting a directory fails
    DeleteDirectoryError {
        directory: PathBuf,
    },

    /// Thrown when deleting a file fails
    DeleteFileError {
        file: PathBuf,
    },

    DeprecatedCommandError {
        command: String,
        advice: String,
    },

    DownloadToolNetworkError {
        tool: tool::Spec,
        from_url: String,
    },

    /// Thrown when unable to execute a hook command
    ExecuteHookError {
        command: String,
    },

    /// Thrown when `volta.extends` keys result in an infinite cycle
    ExtensionCycleError {
        paths: Vec<PathBuf>,
        duplicate: PathBuf,
    },

    /// Thrown when determining the path to an extension manifest fails
    ExtensionPathError {
        path: PathBuf,
    },

    /// Thrown when a hook command returns a non-zero exit code
    HookCommandFailed {
        command: String,
    },

    /// Thrown when a hook contains multiple fields (prefix, template, or bin)
    HookMultipleFieldsSpecified,

    /// Thrown when a hook doesn't contain any of the known fields (prefix, template, or bin)
    HookNoFieldsSpecified,

    /// Thrown when determining the path to a hook fails
    HookPathError {
        command: String,
    },

    /// Thrown when determining the name of a newly-installed package fails
    InstalledPackageNameError,

    InvalidHookCommand {
        command: String,
    },

    /// Thrown when output from a hook command could not be read
    InvalidHookOutput {
        command: String,
    },

    /// Thrown when a user does e.g. `volta install node 12` instead of
    /// `volta install node@12`.
    InvalidInvocation {
        action: String,
        name: String,
        version: String,
    },

    /// Thrown when a user does e.g. `volta install 12` instead of
    /// `volta install node@12`.
    InvalidInvocationOfBareVersion {
        action: String,
        version: String,
    },

    /// Thrown when a format other than "npm" or "github" is given for yarn.index in the hooks
    InvalidRegistryFormat {
        format: String,
    },

    /// Thrown when a tool name is invalid per npm's rules.
    InvalidToolName {
        name: String,
        errors: Vec<String>,
    },

    /// Thrown when unable to acquire a lock on the Volta directory
    LockAcquireError,

    /// Thrown when pinning or installing npm@bundled and couldn't detect the bundled version
    NoBundledNpm {
        command: String,
    },

    /// Thrown when pnpm is not set at the command-line
    NoCommandLinePnpm,

    /// Thrown when Yarn is not set at the command-line
    NoCommandLineYarn,

    /// Thrown when a user tries to install a Yarn or npm version before installing a Node version.
    NoDefaultNodeVersion {
        tool: String,
    },

    /// Thrown when there is no Node version matching a requested semver specifier.
    NodeVersionNotFound {
        matching: String,
    },

    NoHomeEnvironmentVar,

    /// Thrown when the install dir could not be determined
    NoInstallDir,

    NoLocalDataDir,

    /// Thrown when a user tries to pin a npm, pnpm, or Yarn version before pinning a Node version.
    NoPinnedNodeVersion {
        tool: String,
    },

    /// Thrown when the platform (Node version) could not be determined
    NoPlatform,

    /// Thrown when parsing the project manifest and there is a `"volta"` key without Node
    NoProjectNodeInManifest,

    /// Thrown when Yarn is not set in a project
    NoProjectYarn,

    /// Thrown when pnpm is not set in a project
    NoProjectPnpm,

    /// Thrown when no shell profiles could be found
    NoShellProfile {
        env_profile: String,
        bin_dir: PathBuf,
    },

    /// Thrown when the user tries to pin Node or Yarn versions outside of a package.
    NotInPackage,

    /// Thrown when default Yarn is not set
    NoDefaultYarn,

    /// Thrown when default pnpm is not set
    NoDefaultPnpm,

    /// Thrown when `npm link` is called with a package that isn't available
    NpmLinkMissingPackage {
        package: String,
    },

    /// Thrown when `npm link` is called with a package that was not installed / linked with npm
    NpmLinkWrongManager {
        package: String,
    },

    /// Thrown when there is no npm version matching the requested Semver/Tag
    NpmVersionNotFound {
        matching: String,
    },

    NpxNotAvailable {
        version: String,
    },

    /// Thrown when the command to install a global package is not successful
    PackageInstallFailed {
        package: String,
    },

    /// Thrown when parsing the package manifest fails
    PackageManifestParseError {
        package: String,
    },

    /// Thrown when reading the package manifest fails
    PackageManifestReadError {
        package: String,
    },

    /// Thrown when a specified package could not be found on the npm registry
    PackageNotFound {
        package: String,
    },

    /// Thrown when parsing a package manifest fails
    PackageParseError {
        file: PathBuf,
    },

    /// Thrown when reading a package manifest fails
    PackageReadError {
        file: PathBuf,
    },

    /// Thrown when a package has been unpacked but is not formed correctly.
    PackageUnpackError,

    /// Thrown when writing a package manifest fails
    PackageWriteError {
        file: PathBuf,
    },

    /// Thrown when unable to parse a bin config file
    ParseBinConfigError,

    /// Thrown when unable to parse a hooks.json file
    ParseHooksError {
        file: PathBuf,
    },

    /// Thrown when unable to parse the node index cache
    ParseNodeIndexCacheError,

    /// Thrown when unable to parse the node index
    ParseNodeIndexError {
        from_url: String,
    },

    /// Thrown when unable to parse the node index cache expiration
    ParseNodeIndexExpiryError,

    /// Thrown when unable to parse the npm manifest file from a node install
    ParseNpmManifestError,

    /// Thrown when unable to parse a package configuration
    ParsePackageConfigError,

    /// Thrown when unable to parse the platform.json file
    ParsePlatformError,

    /// Thrown when unable to parse the RECURSION_ENV_VAR value
    ParseRecursionEnvError,

    /// Thrown when unable to parse a tool spec (`<tool>[@<version>]`)
    ParseToolSpecError {
        tool_spec: String,
    },

    /// Thrown when persisting an archive to the inventory fails
    PersistInventoryError {
        tool: String,
    },

    /// Thrown when there is no pnpm version matching a requested semver specifier.
    PnpmVersionNotFound {
        matching: String,
    },

    /// Thrown when executing a project-local binary fails
    ProjectLocalBinaryExecError {
        command: String,
    },

    /// Thrown when a project-local binary could not be found
    ProjectLocalBinaryNotFound {
        command: String,
    },

    /// Thrown when a publish hook contains both the url and bin fields
    PublishHookBothUrlAndBin,

    /// Thrown when a publish hook contains neither url nor bin fields
    PublishHookNeitherUrlNorBin,

    /// Thrown when there was an error reading the user bin directory
    ReadBinConfigDirError {
        dir: PathBuf,
    },

    /// Thrown when there was an error reading the config for a binary
    ReadBinConfigError {
        file: PathBuf,
    },

    /// Thrown when unable to read the default npm version file
    ReadDefaultNpmError {
        file: PathBuf,
    },

    /// Thrown when unable to read the contents of a directory
    ReadDirError {
        dir: PathBuf,
    },

    /// Thrown when there was an error opening a hooks.json file
    ReadHooksError {
        file: PathBuf,
    },

    /// Thrown when there was an error reading the Node Index Cache
    ReadNodeIndexCacheError {
        file: PathBuf,
    },

    /// Thrown when there was an error reading the Node Index Cache Expiration
    ReadNodeIndexExpiryError {
        file: PathBuf,
    },

    /// Thrown when there was an error reading the npm manifest file
    ReadNpmManifestError,

    /// Thrown when there was an error reading a package configuration file
    ReadPackageConfigError {
        file: PathBuf,
    },

    /// Thrown when there was an error opening the user platform file
    ReadPlatformError {
        file: PathBuf,
    },

    /// Thrown when recursion limit is reached
    RecursionLimit,

    /// Thrown when unable to read the user Path environment variable from the registry
    #[cfg(windows)]
    ReadUserPathError,

    /// Thrown when the public registry for Node or Yarn could not be downloaded.
    RegistryFetchError {
        tool: String,
        from_url: String,
    },

    /// Thrown when the shim binary is called directly, not through a symlink
    RunShimDirectly,

    /// Thrown when there was an error setting a tool to executable
    SetToolExecutable {
        tool: String,
    },

    /// Thrown when there was an error copying an unpacked tool to the image directory
    SetupToolImageError {
        tool: String,
        version: String,
        dir: PathBuf,
    },

    /// Thrown when Volta is unable to create a shim
    ShimCreateError {
        name: String,
    },

    /// Thrown when Volta is unable to remove a shim
    ShimRemoveError {
        name: String,
    },

    /// Thrown when serializing a bin config to JSON fails
    StringifyBinConfigError,

    /// Thrown when serializing a package config to JSON fails
    StringifyPackageConfigError,

    /// Thrown when serializing the platform to JSON fails
    StringifyPlatformError,

    /// Thrown when a given feature has not yet been implemented
    Unimplemented {
        feature: String,
    },

    /// Thrown when unpacking an archive (tarball or zip) fails
    UnpackArchiveError {
        tool: String,
        version: String,
    },

    /// Thrown when a package to upgrade was not found
    UpgradePackageNotFound {
        package: String,
        manager: PackageManager,
    },

    /// Thrown when a package to upgrade was installed with a different package manager
    UpgradePackageWrongManager {
        package: String,
        manager: PackageManager,
    },

    VersionParseError {
        version: String,
    },

    /// Thrown when there was an error writing a bin config file
    WriteBinConfigError {
        file: PathBuf,
    },

    /// Thrown when there was an error writing the default npm to file
    WriteDefaultNpmError {
        file: PathBuf,
    },

    /// Thrown when there was an error writing the npm launcher
    WriteLauncherError {
        tool: String,
    },

    /// Thrown when there was an error writing the node index cache
    WriteNodeIndexCacheError {
        file: PathBuf,
    },

    /// Thrown when there was an error writing the node index expiration
    WriteNodeIndexExpiryError {
        file: PathBuf,
    },

    /// Thrown when there was an error writing a package config
    WritePackageConfigError {
        file: PathBuf,
    },

    /// Thrown when writing the platform.json file fails
    WritePlatformError {
        file: PathBuf,
    },

    /// Thrown when unable to write the user PATH environment variable
    #[cfg(windows)]
    WriteUserPathError,

    /// Thrown when a user attempts to install a version of Yarn2
    Yarn2NotSupported,

    /// Thrown when there is an error fetching the latest version of Yarn
    YarnLatestFetchError {
        from_url: String,
    },

    /// Thrown when there is no Yarn version matching a requested semver specifier.
    YarnVersionNotFound {
        matching: String,
    },
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ErrorKind::BinaryAlreadyInstalled {
                bin_name,
                existing_package,
                new_package,
            } => write!(
                f,
                "Executable '{}' is already installed by {}

Please remove {} before installing {}",
                bin_name, existing_package, existing_package, new_package
            ),
            ErrorKind::BinaryExecError => write!(
                f,
                "Could not execute command.

See `volta help install` and `volta help pin` for info about making tools available."
            ),
            ErrorKind::BinaryNotFound { name } => write!(
                f,
                r#"Could not find executable "{}"

Use `volta install` to add a package to your toolchain (see `volta help install` for more info)."#,
                name
            ),
            ErrorKind::BuildPathError => write!(
                f,
                "Could not create execution environment.

Please ensure your PATH is valid."
            ),
            ErrorKind::BypassError { command } => write!(
                f,
                "Could not execute command '{}'

VOLTA_BYPASS is enabled, please ensure that the command exists on your system or unset VOLTA_BYPASS",
                command,
            ),
            ErrorKind::CannotFetchPackage { package } => write!(
                f,
                "Fetching packages without installing them is not supported.

Use `volta install {}` to update the default version.",
                package
            ),
            ErrorKind::CannotPinPackage { package } => write!(
                f,
                "Only node and yarn can be pinned in a project

Use `npm install` or `yarn add` to select a version of {} for this project.",
                package
            ),
            ErrorKind::CompletionsOutFileError { path } => write!(
                f,
                "Completions file `{}` already exists.

Please remove the file or pass `-f` or `--force` to override.",
                path.display()
            ),
            ErrorKind::ContainingDirError { path } => write!(
                f,
                "Could not create the containing directory for {}

{}",
                path.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::CouldNotDetermineTool => write!(
                f,
                "Could not determine tool name

{}",
                REPORT_BUG_CTA
            ),
            ErrorKind::CouldNotStartMigration => write!(
                f,
                "Could not start migration process to upgrade your Volta directory.

Please ensure you have 'volta-migrate' on your PATH and run it directly."
            ),
            ErrorKind::CreateDirError { dir } => write!(
                f,
                "Could not create directory {}

Please ensure that you have the correct permissions.",
                dir.display()
            ),
            ErrorKind::CreateLayoutFileError { file } => write!(
                f,
                "Could not create layout file {}

{}",
                file.display(), PERMISSIONS_CTA
            ),
            ErrorKind::CreateSharedLinkError { name } => write!(
                f,
                "Could not create shared environment for package '{}'

{}",
                name, PERMISSIONS_CTA
            ),
            ErrorKind::CreateTempDirError { in_dir } => write!(
                f,
                "Could not create temporary directory
in {}

{}",
                in_dir.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::CreateTempFileError { in_dir } => write!(
                f,
                "Could not create temporary file
in {}

{}",
                in_dir.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::CurrentDirError => write!(
                f,
                "Could not determine current directory

Please ensure that you have the correct permissions."
            ),
            ErrorKind::DeleteDirectoryError { directory } => write!(
                f,
                "Could not remove directory
at {}

{}",
                directory.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::DeleteFileError { file } => write!(
                f,
                "Could not remove file
at {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::DeprecatedCommandError { command, advice } => {
                write!(f, "The subcommand `{}` is deprecated.\n{}", command, advice)
            }
            ErrorKind::DownloadToolNetworkError { tool, from_url } => write!(
                f,
                "Could not download {}
from {}

Please verify your internet connection and ensure the correct version is specified.",
                tool, from_url
            ),
            ErrorKind::ExecuteHookError { command } => write!(
                f,
                "Could not execute hook command: '{}'

Please ensure that the correct command is specified.",
                command
            ),
            ErrorKind::ExtensionCycleError { paths, duplicate } => {
                // Detected infinite loop in project workspace:
                //
                // --> /home/user/workspace/project/package.json
                //     /home/user/workspace/package.json
                // --> /home/user/workspace/project/package.json
                //
                // Please ensure that project workspaces do not depend on each other.
                f.write_str("Detected infinite loop in project workspace:\n\n")?;

                for path in paths {
                    if path == duplicate {
                        f.write_str("--> ")?;
                    } else {
                        f.write_str("    ")?;
                    }

                    writeln!(f, "{}", path.display())?;
                }

                writeln!(f, "--> {}", duplicate.display())?;
                writeln!(f)?;

                f.write_str("Please ensure that project workspaces do not depend on each other.")
            }
            ErrorKind::ExtensionPathError { path } => write!(
                f,
                "Could not determine path to project workspace: '{}'

Please ensure that the file exists and is accessible.",
                path.display(),
            ),
            ErrorKind::HookCommandFailed { command } => write!(
                f,
                "Hook command '{}' indicated a failure.

Please verify the requested tool and version.",
                command
            ),
            ErrorKind::HookMultipleFieldsSpecified => write!(
                f,
                "Hook configuration includes multiple hook types.

Please include only one of 'bin', 'prefix', or 'template'"
            ),
            ErrorKind::HookNoFieldsSpecified => write!(
                f,
                "Hook configuration includes no hook types.

Please include one of 'bin', 'prefix', or 'template'"
            ),
            ErrorKind::HookPathError { command } => write!(
                f,
                "Could not determine path to hook command: '{}'

Please ensure that the correct command is specified.",
                command
            ),
            ErrorKind::InstalledPackageNameError => write!(
                f,
                "Could not determine the name of the package that was just installed.

{}",
                REPORT_BUG_CTA
            ),
            ErrorKind::InvalidHookCommand { command } => write!(
                f,
                "Invalid hook command: '{}'

Please ensure that the correct command is specified.",
                command
            ),
            ErrorKind::InvalidHookOutput { command } => write!(
                f,
                "Could not read output from hook command: '{}'

Please ensure that the command output is valid UTF-8 text.",
                command
            ),

            ErrorKind::InvalidInvocation {
                action,
                name,
                version,
            } => {
                let error = format!(
                    "`volta {action} {name} {version}` is not supported.",
                    action = action,
                    name = name,
                    version = version
                );

                let call_to_action = format!(
"To {action} '{name}' version '{version}', please run `volta {action} {formatted}`. \
To {action} the packages '{name}' and '{version}', please {action} them in separate commands, or with explicit versions.",
                    action=action,
                    name=name,
                    version=version,
                    formatted=tool_version(name, version)
                );

                let wrapped_cta = match text_width() {
                    Some(width) => fill(&call_to_action, width),
                    None => call_to_action,
                };

                write!(f, "{}\n\n{}", error, wrapped_cta)
            }

            ErrorKind::InvalidInvocationOfBareVersion {
                action,
                version,
            } => {
                let error = format!(
                    "`volta {action} {version}` is not supported.",
                    action = action,
                    version = version
                );

                let call_to_action = format!(
"To {action} node version '{version}', please run `volta {action} {formatted}`. \
To {action} the package '{version}', please use an explicit version such as '{version}@latest'.",
                    action=action,
                    version=version,
                    formatted=tool_version("node", version)
                );

                let wrapped_cta = match text_width() {
                    Some(width) => fill(&call_to_action, width),
                    None => call_to_action,
                };

                write!(f, "{}\n\n{}", error, wrapped_cta)
            }

            ErrorKind::InvalidRegistryFormat { format } => write!(
                f,
                "Unrecognized index registry format: '{}'

Please specify either 'npm' or 'github' for the format.",
format
            ),

            ErrorKind::InvalidToolName { name, errors } => {
                let indentation = "    ";
                let wrapped = match text_width() {
                    Some(width) => fill(&errors.join("\n"), width - indentation.len()),
                    None => errors.join("\n"),
                };
                let formatted_errs = indent(&wrapped, indentation);

                let call_to_action = if errors.len() > 1 {
                    "Please fix the following errors:"
                } else {
                    "Please fix the following error:"
                };

                write!(
                    f,
                    "Invalid tool name `{}`\n\n{}\n{}",
                    name, call_to_action, formatted_errs
                )
            }
            // Note: No CTA as this error is purely informational and shouldn't be exposed to the user
            ErrorKind::LockAcquireError => write!(
                f,
                "Unable to acquire lock on Volta directory"
            ),
            ErrorKind::NoBundledNpm { command } => write!(
                f,
                "Could not detect bundled npm version.

Please ensure you have a Node version selected with `volta {} node` (see `volta help {0}` for more info).",
                command
            ),
            ErrorKind::NoCommandLinePnpm => write!(
                f,
                "No pnpm version specified.

Use `volta run --pnpm` to select a version (see `volta help run` for more info)."
            ),
            ErrorKind::NoCommandLineYarn => write!(
                f,
                "No Yarn version specified.

Use `volta run --yarn` to select a version (see `volta help run` for more info)."
            ),
            ErrorKind::NoDefaultNodeVersion { tool } => write!(
                f,
                "Cannot install {} because the default Node version is not set.

Use `volta install node` to select a default Node first, then install a {0} version.",
                                tool
            ),
            ErrorKind::NodeVersionNotFound { matching } => write!(
                f,
                r#"Could not find Node version matching "{}" in the version registry.

Please verify that the version is correct."#,
                matching
            ),
            ErrorKind::NoHomeEnvironmentVar => write!(
                f,
                "Could not determine home directory.

Please ensure the environment variable 'HOME' is set."
            ),
            ErrorKind::NoInstallDir => write!(
                f,
                "Could not determine Volta install directory.

Please ensure Volta was installed correctly"
            ),
            ErrorKind::NoLocalDataDir => write!(
                f,
                "Could not determine LocalAppData directory.

Please ensure the directory is available."
            ),
            ErrorKind::NoPinnedNodeVersion { tool } => write!(
                f,
                "Cannot pin {} because the Node version is not pinned in this project.

Use `volta pin node` to pin Node first, then pin a {0} version.",
                tool
            ),
            ErrorKind::NoPlatform => write!(
                f,
                "Node is not available.

To run any Node command, first set a default version using `volta install node`"
            ),
            ErrorKind::NoProjectNodeInManifest => write!(
                f,
                "No Node version found in this project.

Use `volta pin node` to select a version (see `volta help pin` for more info)."
            ),
            ErrorKind::NoProjectPnpm => write!(
                f,
                "No pnpm version found in this project.

Use `volta pin pnpm` to select a version (see `volta help pin` for more info)."
            ),
            ErrorKind::NoProjectYarn => write!(
                f,
                "No Yarn version found in this project.

Use `volta pin yarn` to select a version (see `volta help pin` for more info)."
            ),
            ErrorKind::NoShellProfile { env_profile, bin_dir } => write!(
                f,
                "Could not locate user profile.
Tried $PROFILE ({}), ~/.bashrc, ~/.bash_profile, ~/.zshenv ~/.zshrc, ~/.profile, and ~/.config/fish/config.fish

Please create one of these and try again; or you can edit your profile manually to add '{}' to your PATH",
                env_profile, bin_dir.display()
            ),
            ErrorKind::NotInPackage => write!(
                f,
                "Not in a node package.

Use `volta install` to select a default version of a tool."
            ),
            ErrorKind::NoDefaultPnpm => write!(
                f,
                "pnpm is not available.

Use `volta install pnpm` to select a default version (see `volta help install` for more info)."
            ),
            ErrorKind::NoDefaultYarn => write!(
                f,
                "Yarn is not available.

Use `volta install yarn` to select a default version (see `volta help install` for more info)."
            ),
            ErrorKind::NpmLinkMissingPackage { package } => write!(
                f,
                "Could not locate the package '{}'

Please ensure it is available by running `npm link` in its source directory.",
                package
            ),
            ErrorKind::NpmLinkWrongManager { package } => write!(
                f,
                "The package '{}' was not installed using npm and cannot be linked with `npm link`

Please ensure it is linked with `npm link` or installed with `npm i -g {0}`.",
                package
            ),
            ErrorKind::NpmVersionNotFound { matching } => write!(
                f,
                r#"Could not find Node version matching "{}" in the version registry.

Please verify that the version is correct."#,
                matching
            ),
            ErrorKind::NpxNotAvailable { version } => write!(
                f,
                "'npx' is only available with npm >= 5.2.0

This project is configured to use version {} of npm.",
                version
            ),
            ErrorKind::PackageInstallFailed { package } => write!(
                f,
                "Could not install package '{}'

Please confirm the package is valid and run with `--verbose` for more diagnostics.",
                package
            ),
            ErrorKind::PackageManifestParseError { package } => write!(
                f,
                "Could not parse package.json manifest for {}

Please ensure the package includes a valid manifest file.",
                package
            ),
            ErrorKind::PackageManifestReadError { package } => write!(
                f,
                "Could not read package.json manifest for {}

Please ensure the package includes a valid manifest file.",
                package
            ),
            ErrorKind::PackageNotFound { package } => write!(
                f,
                "Could not find '{}' in the package registry.

Please verify the requested package is correct.",
                package
            ),
            ErrorKind::PackageParseError { file } => write!(
                f,
                "Could not parse project manifest
at {}

Please ensure that the file is correctly formatted.",
                file.display()
            ),
            ErrorKind::PackageReadError { file } => write!(
                f,
                "Could not read project manifest
from {}

Please ensure that the file exists.",
                file.display()
            ),
            ErrorKind::PackageUnpackError => write!(
                f,
                "Could not determine package directory layout.

Please ensure the package is correctly formatted."
            ),
            ErrorKind::PackageWriteError { file } => write!(
                f,
                "Could not write project manifest
to {}

Please ensure you have correct permissions.",
                file.display()
            ),
            ErrorKind::ParseBinConfigError => write!(
                f,
                "Could not parse executable configuration file.

{}",
                REPORT_BUG_CTA
            ),
            ErrorKind::ParseHooksError { file } => write!(
                f,
                "Could not parse hooks configuration file.
from {}

Please ensure the file is correctly formatted.",
                file.display()
            ),
            ErrorKind::ParseNodeIndexCacheError => write!(
                f,
                "Could not parse Node index cache file.

{}",
                REPORT_BUG_CTA
            ),
            ErrorKind::ParseNodeIndexError { from_url } => write!(
                f,
                "Could not parse Node version index
from {}

Please verify your internet connection.",
                from_url
            ),
            ErrorKind::ParseNodeIndexExpiryError => write!(
                f,
                "Could not parse Node index cache expiration file.

{}",
                REPORT_BUG_CTA
            ),
            ErrorKind::ParseNpmManifestError => write!(
                f,
                "Could not parse package.json file for bundled npm.

Please ensure the version of Node is correct."
            ),
            ErrorKind::ParsePackageConfigError => write!(
                f,
                "Could not parse package configuration file.

{}",
                REPORT_BUG_CTA
            ),
            ErrorKind::ParsePlatformError => write!(
                f,
                "Could not parse platform settings file.

{}",
                REPORT_BUG_CTA
            ),
            ErrorKind::ParseRecursionEnvError => write!(
                f,
                "Could not parse RECURSION_ENV_VAR value."
            ),
            ErrorKind::ParseToolSpecError { tool_spec } => write!(
                f,
                "Could not parse tool spec `{}`

Please supply a spec in the format `<tool name>[@<version>]`.",
                tool_spec
            ),
            ErrorKind::PersistInventoryError { tool } => write!(
                f,
                "Could not store {} archive in inventory cache

{}",
                tool, PERMISSIONS_CTA
            ),
            ErrorKind::PnpmVersionNotFound { matching } => write!(
                f,
                r#"Could not find pnpm version matching "{}" in the version registry.

Please verify that the version is correct."#,
                matching
            ),
            ErrorKind::ProjectLocalBinaryExecError { command } => write!(
                f,
                "Could not execute `{}`

Please ensure you have correct permissions to access the file.",
                command
            ),
            ErrorKind::ProjectLocalBinaryNotFound { command } => write!(
                f,
                "Could not locate executable `{}` in your project.

Please ensure that all project dependencies are installed with `npm install` or `yarn install`",
                command
            ),
            ErrorKind::PublishHookBothUrlAndBin => write!(
                f,
                "Publish hook configuration includes both hook types.

Please include only one of 'bin' or 'url'"
            ),
            ErrorKind::PublishHookNeitherUrlNorBin => write!(
                f,
                "Publish hook configuration includes no hook types.

Please include one of 'bin' or 'url'"
            ),
            ErrorKind::ReadBinConfigDirError { dir } => write!(
                f,
                "Could not read executable metadata directory
at {}

{}",
                dir.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::ReadBinConfigError { file } => write!(
                f,
                "Could not read executable configuration
from {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::ReadDefaultNpmError { file } => write!(
                f,
                "Could not read default npm version
from {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::ReadDirError { dir } => write!(
                f,
                "Could not read contents from directory {}

{}",
                dir.display(), PERMISSIONS_CTA
            ),
            ErrorKind::ReadHooksError { file } => write!(
                f,
                "Could not read hooks file
from {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::ReadNodeIndexCacheError { file } => write!(
                f,
                "Could not read Node index cache
from {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::ReadNodeIndexExpiryError { file } => write!(
                f,
                "Could not read Node index cache expiration
from {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::ReadNpmManifestError => write!(
                f,
                "Could not read package.json file for bundled npm.

Please ensure the version of Node is correct."
            ),
            ErrorKind::ReadPackageConfigError { file } => write!(
                f,
                "Could not read package configuration file
from {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::ReadPlatformError { file } => write!(
                f,
                "Could not read default platform file
from {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::RecursionLimit => write!(
                f,
                "Recursive call limit reached."
            ),
            #[cfg(windows)]
            ErrorKind::ReadUserPathError => write!(
                f,
                "Could not read user Path environment variable.

Please ensure you have access to the your environment variables."
            ),
            ErrorKind::RegistryFetchError { tool, from_url } => write!(
                f,
                "Could not download {} version registry
from {}

Please verify your internet connection.",
                tool, from_url
            ),
            ErrorKind::RunShimDirectly => write!(
                f,
                "'volta-shim' should not be called directly.

Please use the existing shims provided by Volta (node, yarn, etc.) to run tools."
            ),
            ErrorKind::SetToolExecutable { tool } => write!(
                f,
                r#"Could not set "{}" to executable

{}"#,
                tool, PERMISSIONS_CTA
            ),
            ErrorKind::SetupToolImageError { tool, version, dir } => write!(
                f,
                "Could not create environment for {} v{}
at {}

{}",
                tool,
                version,
                dir.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::ShimCreateError { name } => write!(
                f,
                r#"Could not create shim for "{}"

{}"#,
                name, PERMISSIONS_CTA
            ),
            ErrorKind::ShimRemoveError { name } => write!(
                f,
                r#"Could not remove shim for "{}"

{}"#,
                name, PERMISSIONS_CTA
            ),
            ErrorKind::StringifyBinConfigError => write!(
                f,
                "Could not serialize executable configuration.

{}",
                REPORT_BUG_CTA
            ),
            ErrorKind::StringifyPackageConfigError => write!(
                f,
                "Could not serialize package configuration.

{}",
                REPORT_BUG_CTA
            ),
            ErrorKind::StringifyPlatformError => write!(
                f,
                "Could not serialize platform settings.

{}",
                REPORT_BUG_CTA
            ),
            ErrorKind::Unimplemented { feature } => {
                write!(f, "{} is not supported yet.", feature)
            }
            ErrorKind::UnpackArchiveError { tool, version } => write!(
                f,
                "Could not unpack {} v{}

Please ensure the correct version is specified.",
                tool, version
            ),
            ErrorKind::UpgradePackageNotFound { package, manager } => write!(
                f,
                r#"Could not locate the package '{}' to upgrade.

Please ensure it is installed with `{} {0}`"#,
                package,
                match manager {
                    PackageManager::Npm => "npm i -g",
                    PackageManager::Pnpm => "pnpm add -g",
                    PackageManager::Yarn => "yarn global add",
                }
            ),
            ErrorKind::UpgradePackageWrongManager { package, manager } => {
                let (name, command) = match manager {
                    PackageManager::Npm => ("npm", "npm update -g"),
                    PackageManager::Pnpm => ("pnpm", "pnpm update -g"),
                    PackageManager::Yarn => ("Yarn", "yarn global upgrade"),
                };
                write!(
                    f,
                    r#"The package '{}' was installed using {}.

To upgrade it, please use the command `{} {0}`"#,
                    package, name, command
                )
            }
            ErrorKind::VersionParseError { version } => write!(
                f,
                r#"Could not parse version "{}"

Please verify the intended version."#,
                version
            ),
            ErrorKind::WriteBinConfigError { file } => write!(
                f,
                "Could not write executable configuration
to {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::WriteDefaultNpmError { file } => write!(
                f,
                "Could not write bundled npm version
to {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::WriteLauncherError { tool } => write!(
                f,
                "Could not set up launcher for {}

This is most likely an intermittent failure, please try again.",
                tool
            ),
            ErrorKind::WriteNodeIndexCacheError { file } => write!(
                f,
                "Could not write Node index cache
to {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::WriteNodeIndexExpiryError { file } => write!(
                f,
                "Could not write Node index cache expiration
to {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::WritePackageConfigError { file } => write!(
                f,
                "Could not write package configuration
to {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            ErrorKind::WritePlatformError { file } => write!(
                f,
                "Could not save platform settings
to {}

{}",
                file.display(),
                PERMISSIONS_CTA
            ),
            #[cfg(windows)]
            ErrorKind::WriteUserPathError => write!(
                f,
                "Could not write Path environment variable.

Please ensure you have permissions to edit your environment variables."
            ),
            ErrorKind::Yarn2NotSupported => write!(
                f,
                "Yarn version 2 is not recommended for use, and not supported by Volta.

Please use version 3 or greater instead."
            ),
            ErrorKind::YarnLatestFetchError { from_url } => write!(
                f,
                "Could not fetch latest version of Yarn
from {}

Please verify your internet connection.",
                from_url
            ),
            ErrorKind::YarnVersionNotFound { matching } => write!(
                f,
                r#"Could not find Yarn version matching "{}" in the version registry.

Please verify that the version is correct."#,
                matching
            ),
        }
    }
}

impl ErrorKind {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            ErrorKind::BinaryAlreadyInstalled { .. } => ExitCode::FileSystemError,
            ErrorKind::BinaryExecError => ExitCode::ExecutionFailure,
            ErrorKind::BinaryNotFound { .. } => ExitCode::ExecutableNotFound,
            ErrorKind::BuildPathError => ExitCode::EnvironmentError,
            ErrorKind::BypassError { .. } => ExitCode::ExecutionFailure,
            ErrorKind::CannotFetchPackage { .. } => ExitCode::InvalidArguments,
            ErrorKind::CannotPinPackage { .. } => ExitCode::InvalidArguments,
            ErrorKind::CompletionsOutFileError { .. } => ExitCode::InvalidArguments,
            ErrorKind::ContainingDirError { .. } => ExitCode::FileSystemError,
            ErrorKind::CouldNotDetermineTool => ExitCode::UnknownError,
            ErrorKind::CouldNotStartMigration => ExitCode::EnvironmentError,
            ErrorKind::CreateDirError { .. } => ExitCode::FileSystemError,
            ErrorKind::CreateLayoutFileError { .. } => ExitCode::FileSystemError,
            ErrorKind::CreateSharedLinkError { .. } => ExitCode::FileSystemError,
            ErrorKind::CreateTempDirError { .. } => ExitCode::FileSystemError,
            ErrorKind::CreateTempFileError { .. } => ExitCode::FileSystemError,
            ErrorKind::CurrentDirError => ExitCode::EnvironmentError,
            ErrorKind::DeleteDirectoryError { .. } => ExitCode::FileSystemError,
            ErrorKind::DeleteFileError { .. } => ExitCode::FileSystemError,
            ErrorKind::DeprecatedCommandError { .. } => ExitCode::InvalidArguments,
            ErrorKind::DownloadToolNetworkError { .. } => ExitCode::NetworkError,
            ErrorKind::ExecuteHookError { .. } => ExitCode::ExecutionFailure,
            ErrorKind::ExtensionCycleError { .. } => ExitCode::ConfigurationError,
            ErrorKind::ExtensionPathError { .. } => ExitCode::FileSystemError,
            ErrorKind::HookCommandFailed { .. } => ExitCode::ConfigurationError,
            ErrorKind::HookMultipleFieldsSpecified => ExitCode::ConfigurationError,
            ErrorKind::HookNoFieldsSpecified => ExitCode::ConfigurationError,
            ErrorKind::HookPathError { .. } => ExitCode::ConfigurationError,
            ErrorKind::InstalledPackageNameError => ExitCode::UnknownError,
            ErrorKind::InvalidHookCommand { .. } => ExitCode::ExecutableNotFound,
            ErrorKind::InvalidHookOutput { .. } => ExitCode::ExecutionFailure,
            ErrorKind::InvalidInvocation { .. } => ExitCode::InvalidArguments,
            ErrorKind::InvalidInvocationOfBareVersion { .. } => ExitCode::InvalidArguments,
            ErrorKind::InvalidRegistryFormat { .. } => ExitCode::ConfigurationError,
            ErrorKind::InvalidToolName { .. } => ExitCode::InvalidArguments,
            ErrorKind::LockAcquireError => ExitCode::FileSystemError,
            ErrorKind::NoBundledNpm { .. } => ExitCode::ConfigurationError,
            ErrorKind::NoCommandLinePnpm => ExitCode::ConfigurationError,
            ErrorKind::NoCommandLineYarn => ExitCode::ConfigurationError,
            ErrorKind::NoDefaultNodeVersion { .. } => ExitCode::ConfigurationError,
            ErrorKind::NodeVersionNotFound { .. } => ExitCode::NoVersionMatch,
            ErrorKind::NoHomeEnvironmentVar => ExitCode::EnvironmentError,
            ErrorKind::NoInstallDir => ExitCode::EnvironmentError,
            ErrorKind::NoLocalDataDir => ExitCode::EnvironmentError,
            ErrorKind::NoPinnedNodeVersion { .. } => ExitCode::ConfigurationError,
            ErrorKind::NoPlatform => ExitCode::ConfigurationError,
            ErrorKind::NoProjectNodeInManifest => ExitCode::ConfigurationError,
            ErrorKind::NoProjectPnpm => ExitCode::ConfigurationError,
            ErrorKind::NoProjectYarn => ExitCode::ConfigurationError,
            ErrorKind::NoShellProfile { .. } => ExitCode::EnvironmentError,
            ErrorKind::NotInPackage => ExitCode::ConfigurationError,
            ErrorKind::NoDefaultPnpm => ExitCode::ConfigurationError,
            ErrorKind::NoDefaultYarn => ExitCode::ConfigurationError,
            ErrorKind::NpmLinkMissingPackage { .. } => ExitCode::ConfigurationError,
            ErrorKind::NpmLinkWrongManager { .. } => ExitCode::ConfigurationError,
            ErrorKind::NpmVersionNotFound { .. } => ExitCode::NoVersionMatch,
            ErrorKind::NpxNotAvailable { .. } => ExitCode::ExecutableNotFound,
            ErrorKind::PackageInstallFailed { .. } => ExitCode::UnknownError,
            ErrorKind::PackageManifestParseError { .. } => ExitCode::ConfigurationError,
            ErrorKind::PackageManifestReadError { .. } => ExitCode::FileSystemError,
            ErrorKind::PackageNotFound { .. } => ExitCode::InvalidArguments,
            ErrorKind::PackageParseError { .. } => ExitCode::ConfigurationError,
            ErrorKind::PackageReadError { .. } => ExitCode::FileSystemError,
            ErrorKind::PackageUnpackError => ExitCode::ConfigurationError,
            ErrorKind::PackageWriteError { .. } => ExitCode::FileSystemError,
            ErrorKind::ParseBinConfigError => ExitCode::UnknownError,
            ErrorKind::ParseHooksError { .. } => ExitCode::ConfigurationError,
            ErrorKind::ParseToolSpecError { .. } => ExitCode::InvalidArguments,
            ErrorKind::ParseNodeIndexCacheError => ExitCode::UnknownError,
            ErrorKind::ParseNodeIndexError { .. } => ExitCode::NetworkError,
            ErrorKind::ParseNodeIndexExpiryError => ExitCode::UnknownError,
            ErrorKind::ParseNpmManifestError => ExitCode::UnknownError,
            ErrorKind::ParsePackageConfigError => ExitCode::UnknownError,
            ErrorKind::ParsePlatformError => ExitCode::ConfigurationError,
            ErrorKind::ParseRecursionEnvError => ExitCode::UnknownError,
            ErrorKind::PersistInventoryError { .. } => ExitCode::FileSystemError,
            ErrorKind::PnpmVersionNotFound { .. } => ExitCode::NoVersionMatch,
            ErrorKind::ProjectLocalBinaryExecError { .. } => ExitCode::ExecutionFailure,
            ErrorKind::ProjectLocalBinaryNotFound { .. } => ExitCode::FileSystemError,
            ErrorKind::PublishHookBothUrlAndBin => ExitCode::ConfigurationError,
            ErrorKind::PublishHookNeitherUrlNorBin => ExitCode::ConfigurationError,
            ErrorKind::ReadBinConfigDirError { .. } => ExitCode::FileSystemError,
            ErrorKind::ReadBinConfigError { .. } => ExitCode::FileSystemError,
            ErrorKind::ReadDefaultNpmError { .. } => ExitCode::FileSystemError,
            ErrorKind::ReadDirError { .. } => ExitCode::FileSystemError,
            ErrorKind::ReadHooksError { .. } => ExitCode::FileSystemError,
            ErrorKind::ReadNodeIndexCacheError { .. } => ExitCode::FileSystemError,
            ErrorKind::ReadNodeIndexExpiryError { .. } => ExitCode::FileSystemError,
            ErrorKind::ReadNpmManifestError => ExitCode::UnknownError,
            ErrorKind::ReadPackageConfigError { .. } => ExitCode::FileSystemError,
            ErrorKind::ReadPlatformError { .. } => ExitCode::FileSystemError,
            ErrorKind::RecursionLimit => ExitCode::ExecutionFailure,
            #[cfg(windows)]
            ErrorKind::ReadUserPathError => ExitCode::EnvironmentError,
            ErrorKind::RegistryFetchError { .. } => ExitCode::NetworkError,
            ErrorKind::RunShimDirectly => ExitCode::InvalidArguments,
            ErrorKind::SetupToolImageError { .. } => ExitCode::FileSystemError,
            ErrorKind::SetToolExecutable { .. } => ExitCode::FileSystemError,
            ErrorKind::ShimCreateError { .. } => ExitCode::FileSystemError,
            ErrorKind::ShimRemoveError { .. } => ExitCode::FileSystemError,
            ErrorKind::StringifyBinConfigError => ExitCode::UnknownError,
            ErrorKind::StringifyPackageConfigError => ExitCode::UnknownError,
            ErrorKind::StringifyPlatformError => ExitCode::UnknownError,
            ErrorKind::Unimplemented { .. } => ExitCode::UnknownError,
            ErrorKind::UnpackArchiveError { .. } => ExitCode::UnknownError,
            ErrorKind::UpgradePackageNotFound { .. } => ExitCode::ConfigurationError,
            ErrorKind::UpgradePackageWrongManager { .. } => ExitCode::ConfigurationError,
            ErrorKind::VersionParseError { .. } => ExitCode::NoVersionMatch,
            ErrorKind::WriteBinConfigError { .. } => ExitCode::FileSystemError,
            ErrorKind::WriteDefaultNpmError { .. } => ExitCode::FileSystemError,
            ErrorKind::WriteLauncherError { .. } => ExitCode::FileSystemError,
            ErrorKind::WriteNodeIndexCacheError { .. } => ExitCode::FileSystemError,
            ErrorKind::WriteNodeIndexExpiryError { .. } => ExitCode::FileSystemError,
            ErrorKind::WritePackageConfigError { .. } => ExitCode::FileSystemError,
            ErrorKind::WritePlatformError { .. } => ExitCode::FileSystemError,
            #[cfg(windows)]
            ErrorKind::WriteUserPathError => ExitCode::EnvironmentError,
            ErrorKind::Yarn2NotSupported => ExitCode::NoVersionMatch,
            ErrorKind::YarnLatestFetchError { .. } => ExitCode::NetworkError,
            ErrorKind::YarnVersionNotFound { .. } => ExitCode::NoVersionMatch,
        }
    }
}
