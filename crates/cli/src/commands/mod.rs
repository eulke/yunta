//! One module per subcommand, beside the few things more than one of
//! them shares; `main.rs` only parses and dispatches.

pub mod cancel;
pub mod check;
pub mod close;
pub mod doctor;
pub(crate) mod drive;
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

use yunta_render::start::Start;

use crate::output::Mode;
use surco::look::Look;

/// The line a run starts with, as render says it. Under `--quiet`, the id
/// and nothing else; under `--json`, nothing, because the document is the
/// whole answer.
pub(crate) fn started(run_id: &yunta_core::RunId, start: Start, output: Mode, look: &Look) {
    match output {
        Mode::Human => crate::output::say(look, yunta_render::start::line(run_id, start)),
        Mode::Quiet => crate::output::say(look, run_id.to_string()),
        Mode::Json => {}
    }
}
