//! What a scope-expansion event says happened, read as a person reads it.

use crate::events::{Decider, ScopeEvent};
use crate::{ScopeGlob, TaskId};

/// One step of one task's request to work outside its declared scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Happening {
    Expansion { task: TaskId, step: Step },
}

/// Where a request stands: asked for, granted, or turned down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Requested {
        paths: Vec<ScopeGlob>,
    },
    Granted {
        by: Decider,
    },
    Denied {
        by: Decider,
        /// Why, when whoever denied it said. A denial with no reason is
        /// a decision a reader cannot act on, and the log carries that
        /// absence rather than inventing a sentence for it.
        reason: Option<String>,
    },
}

impl From<&ScopeEvent> for Happening {
    fn from(event: &ScopeEvent) -> Self {
        match event {
            ScopeEvent::Requested(p) => Happening::Expansion {
                task: p.task_id.clone(),
                step: Step::Requested {
                    paths: p.paths.clone(),
                },
            },
            ScopeEvent::Granted(p) => Happening::Expansion {
                task: p.task_id.clone(),
                step: Step::Granted {
                    by: p.decided_by.clone(),
                },
            },
            ScopeEvent::Denied(p) => Happening::Expansion {
                task: p.task_id.clone(),
                step: Step::Denied {
                    by: p.decided_by.clone(),
                    reason: p.denial_reason.clone(),
                },
            },
        }
    }
}
