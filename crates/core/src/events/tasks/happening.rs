//! What a task event says happened, read as a person reads it.

use std::collections::BTreeSet;

use crate::events::{DepartsFrom, InTree, Outside, ShapesReadPayload, TaskEvent, TaskStatus};
use crate::TaskId;

/// One thing that happened to a task, or to the plan its tasks come from.
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
    /// Its session declared a criterion no work of the plan can pass.
    Unpassable {
        task: TaskId,
        criterion: String,
        why: String,
    },
    /// The tree its plan was registered against was read for the plan's
    /// shapes: how many were read, how many of them the tree holds, and
    /// how many files no task changes name them.
    ShapesRead {
        shapes: usize,
        held: usize,
        outside: usize,
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
            TaskEvent::CriterionDeclaredUnpassable(p) => Happening::Unpassable {
                task: p.task_id.clone(),
                criterion: p.criterion.clone(),
                why: p.why.clone(),
            },
            TaskEvent::ShapesRead(p) => Happening::ShapesRead {
                shapes: p.shapes.len(),
                held: p
                    .shapes
                    .iter()
                    .filter(|shape| matches!(shape.in_tree, InTree::There { .. }))
                    .count(),
                outside: outside(p),
            },
        }
    }
}

/// How many files no task of the plan changes name one of its shapes, each
/// counted once however many it names.
fn outside(read: &ShapesReadPayload) -> usize {
    let mut files = BTreeSet::new();
    for shape in &read.shapes {
        if let InTree::There {
            outside: Outside::Files { paths },
            ..
        } = &shape.in_tree
        {
            files.extend(paths);
        }
    }
    files.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ShapeRead;
    use crate::hash::ContentHash;

    /// One shape of a plan, as the tree held it.
    fn shape(name: &str, in_tree: InTree) -> ShapeRead {
        ShapeRead {
            name: name.into(),
            owner: TaskId::from_static("task-h"),
            file: "src/lib.rs".into(),
            in_tree,
        }
    }

    /// A shape its file holds, with what names it beyond the plan.
    fn there(outside: Outside) -> InTree {
        InTree::There {
            code: None,
            outside,
        }
    }

    /// Every shape read counts, only those the tree holds are held, and a
    /// file two shapes are named in counts once; a name too common to follow
    /// lists no file, so it adds none.
    #[test]
    fn a_reading_counts_the_shapes_held_and_each_file_beyond_the_plan_once() {
        let other = || Outside::Files {
            paths: vec!["src/other.rs".into()],
        };
        let reading = ShapesReadPayload {
            plan: ContentHash::sha256(b"tasks"),
            at: "0123456789abcdef0123456789abcdef01234567".into(),
            shapes: vec![
                shape("build_it", there(other())),
                shape("Theme", there(other())),
                shape("fresh", InTree::New),
                shape("add", there(Outside::Common { files: 147 })),
            ],
        };
        assert_eq!(
            Happening::from(&TaskEvent::ShapesRead(reading)),
            Happening::ShapesRead {
                shapes: 4,
                held: 3,
                outside: 1,
            }
        );
    }
}
