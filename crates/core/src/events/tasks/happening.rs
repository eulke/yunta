//! What a task event says happened, read as a person reads it.

use crate::events::{DepartsFrom, TaskEvent, TaskStatus};
use crate::TaskId;

/// One thing that happened to a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Happening {
    Registered {
        task: TaskId,
    },
    Moved {
        task: TaskId,
        to: TaskStatus,
    },
    /// Its session asked how its work would be judged.
    Checking {
        task: TaskId,
    },
    /// And was answered: whether it would close, how many of its
    /// criteria were red, and how long the judgement took.
    Checked {
        task: TaskId,
        closes: bool,
        red: usize,
        duration_ms: u64,
    },
    /// Its session departed from the plan.
    Departed {
        task: TaskId,
        from: DepartsFrom,
    },
    /// A person answered that departure.
    Answered {
        task: TaskId,
        accepted: bool,
    },
}

impl From<&TaskEvent> for Happening {
    fn from(event: &TaskEvent) -> Self {
        match event {
            TaskEvent::Registered(p) => Happening::Registered {
                task: p.task_id.clone(),
            },
            TaskEvent::StatusChanged(p) => Happening::Moved {
                task: p.task_id.clone(),
                to: p.new_status,
            },
            TaskEvent::CheckStarted(p) => Happening::Checking {
                task: p.task_id.clone(),
            },
            TaskEvent::CheckAnswered(p) => Happening::Checked {
                task: p.task_id.clone(),
                closes: p.closes,
                red: p.results.iter().filter(|r| r.exit_code != 0).count(),
                duration_ms: p.duration_ms,
            },
            TaskEvent::DeviationDeclared(p) => Happening::Departed {
                task: p.task_id.clone(),
                from: p.from.clone(),
            },
            TaskEvent::DeviationResolved(p) => Happening::Answered {
                task: p.task_id.clone(),
                accepted: p.accepted,
            },
        }
    }
}
