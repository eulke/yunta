//! The run a command takes, as clap reads it off the command line: what a
//! person typed for it, which the API resolves among the runs this machine
//! holds.

use yunta_api::RunRef;
use yunta_core::RunId;

use crate::error::CliError;
use crate::shell::Shell;

/// The run a command takes, as every command that takes one documents
/// it.
#[derive(Debug, Clone, clap::Args)]
pub(crate) struct RunArg {
    /// The run: its handle, its id or any part that starts or ends it,
    /// `last` (this repository's newest) or `needs` (the one waiting on
    /// you).
    run: RunRef,
}

impl RunArg {
    /// The run this names, among the runs this machine holds.
    pub(crate) async fn named(&self, shell: &Shell) -> Result<RunId, CliError> {
        named(shell, &self.run).await
    }
}

/// The run `reference` names, among the runs this machine holds.
pub(crate) async fn named(shell: &Shell, reference: &RunRef) -> Result<RunId, CliError> {
    let project = shell.workspace().project()?;
    Ok(project.find(reference, &shell.stop()).await?)
}
