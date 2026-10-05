//! What a child-run event says happened, read as a person reads it.

use crate::events::{ChildEvent, TerminalState};
use crate::RunId;

/// One thing that happened to a run this run bore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Happening {
    Born(RunId),
    Closed {
        run_id: RunId,
        terminal: TerminalState,
    },
    Iteration {
        iteration: u32,
    },
}

impl From<&ChildEvent> for Happening {
    fn from(event: &ChildEvent) -> Self {
        match event {
            ChildEvent::Created(p) => Happening::Born(p.child_run_id.clone()),
            ChildEvent::Finished(p) => Happening::Closed {
                run_id: p.child_run_id.clone(),
                terminal: p.terminal_state,
            },
            ChildEvent::LoopIteration(p) => Happening::Iteration {
                iteration: p.iteration,
            },
        }
    }
}
