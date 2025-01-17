use volta_core::error::{ExitCode, Fallible};
use volta_core::session::{ActivityKind, Session};
use volta_core::tool::Spec;

use crate::command::Command;

#[derive(clap::Args)]
pub(crate) struct Uninstall {
    /// Tools to uninstall, like `node`, `yarn@latest` or `your-package`.
    #[arg(value_name = "tool[@version]", required = true)]
    tools: Vec<String>,
}

impl Command for Uninstall {
    fn run(self, session: &mut Session) -> Fallible<ExitCode> {
        session.add_event_start(ActivityKind::Uninstall);

        for tool in Spec::from_strings(&self.tools, "uninstall")? {
            tool.resolve(session)?.uninstall(session)?;
        }

        session.add_event_end(ActivityKind::Uninstall, ExitCode::Success);
        Ok(ExitCode::Success)
    }
}
