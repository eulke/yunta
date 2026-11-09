//! One module per subcommand, beside the few things more than one of
//! them shares; `main.rs` only parses and dispatches.

pub mod cancel;
pub mod check;
pub mod close;
pub mod doctor;
pub(crate) mod drive;
pub(crate) mod forge;
pub mod gc;
pub mod init;
pub mod list;
pub mod mcp;
pub mod new;
pub mod open;
pub mod pack;
pub mod rail;
pub mod receipt;
pub mod resolve_gate;
pub mod resume;
pub mod review;
pub mod run;
pub(crate) mod run_ref;
pub mod schema;
pub mod stats;
pub mod status;
pub mod test;
pub mod verify;

use yunta_core::step::NextStep;
use yunta_render::command;

use crate::output::Mode;
use surco::look::Look;

/// The line a run starts with: its whole id — the record a script and a
/// log keep — what is happening to it, and the command that follows it
/// by its handle. Under `--quiet`, the id and nothing else; under `--json`,
/// nothing, because the document is the whole answer.
pub(crate) fn started(run_id: &yunta_core::RunId, doing: &str, output: Mode, look: &Look) {
    match output {
        Mode::Human => crate::output::say(
            look,
            format!(
                "run {run_id}: {doing} (`{}` follows it)",
                command::line(&NextStep::Follow {
                    run: run_id.clone()
                })
            ),
        ),
        Mode::Quiet => crate::output::say(look, run_id.to_string()),
        Mode::Json => {}
    }
}
