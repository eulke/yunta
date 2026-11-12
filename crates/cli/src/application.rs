//! The application at a terminal, which bare `yunta` opens: whether this process may open
//! it, and what it hands it. Stdin and stdout at a terminal that draws, where a person
//! works in it; anywhere else the command prints as it always has.
//!
//! The application is handed what the shell read: a performer over the workspace, the
//! stdout look, the workspace's clock, the system's opener for a page, and the signals
//! that ask the process to end; how it ended is the code the process exits with. A
//! terminal that will not be held is printed to instead; one that fails once the
//! application is open ends the command with the failure said.

use std::io::IsTerminal;
use std::sync::Arc;

use surco::look::Look;
use yunta_tui::{Application, Exit, TerminalFailure};
use yunta_ui::{Performer, Request, State};

use crate::error::CliError;
use crate::output::Outcome;
use crate::shell::Shell;
use crate::surface::TerminalEnv;

/// Whether this process may open the application: stdin and stdout at a terminal, which
/// does not say it draws nothing beyond text.
pub(crate) fn opens_here() -> bool {
    std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && !TerminalEnv::settled().is_dumb()
}

/// The application run at this terminal, `opened` and what it asks as it opens, in `look`:
/// how the process exits once the person leaves or a signal asks it to end; none where the
/// terminal would not be held, which is printed to instead. A terminal that failed once
/// the application was open is the command's failure.
pub(crate) async fn at_this_terminal(
    shell: &Shell,
    look: &Look,
    opened: (State, Vec<Request>),
) -> Result<Option<Outcome>, CliError> {
    let workspace = shell.workspace();
    let performer = Arc::new(Performer::new(workspace.clone()));
    let application = Application::new(
        performer,
        *look,
        workspace.clock.clone(),
        crate::browser::open,
    );
    let asked_to_end = shell.asked_to_end()?;
    Ok(match application.run(opened, asked_to_end).await {
        Ok(Exit::Left) => Some(Outcome::Success),
        Ok(Exit::Interrupted) => Some(Outcome::interrupted()),
        Ok(Exit::AskedToEnd(signal)) => Some(Outcome::ended_by(signal)),
        // A terminal that refuses to be held draws nothing beyond text for this process:
        // what the command prints off a terminal is what it gets.
        Err(TerminalFailure::Unheld(_)) => None,
        Err(TerminalFailure::Lost(source)) => {
            return Err(CliError::Io {
                context: "keep the application open at this terminal".to_string(),
                source,
            })
        }
    })
}
