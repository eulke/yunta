//! What a task event says happened, read as a person reads it.

use crate::events::{TaskEvent, TaskStatus};
use crate::TaskId;

/// One thing that happened to a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Happening {
    Registered { task: TaskId },
    Moved { task: TaskId, to: TaskStatus },
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
        }
    }
}
