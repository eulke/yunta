//! What a finding event says happened, read as a person reads it.

use crate::events::{FindingAnswer, FindingEvent, FindingOperation, FindingSeverity};
use crate::{FindingId, NodeId};

/// One thing that happened to one finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Happening {
    Finding {
        /// The finding this is about; `None` for a call the engine
        /// refused before it could name one.
        id: Option<FindingId>,
        severity: Option<FindingSeverity>,
        title: String,
        change: Change,
    },
}

/// What happened to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Posted,
    Updated,
    Withdrawn {
        reason: String,
    },
    Refused {
        operation: FindingOperation,
        problems: usize,
    },
    /// Another node answered it.
    Answered {
        of: NodeId,
        answer: FindingAnswer,
        why: String,
    },
    /// The criterion it proposes ran on the tree the answering node left.
    Proved {
        of: NodeId,
        cmd: String,
        passed: bool,
    },
    /// A person went on past a gate that showed it.
    Settled {
        of: Option<NodeId>,
    },
}

impl Happening {
    /// What happened to `finding`, said with its severity and title.
    fn of(finding: &crate::events::Finding, change: Change) -> Self {
        Happening::Finding {
            id: Some(finding.id.clone()),
            severity: Some(finding.severity),
            title: finding.title.clone(),
            change,
        }
    }

    /// What happened to the finding `id`, by its id alone.
    fn about(id: Option<&FindingId>, change: Change) -> Self {
        Happening::Finding {
            id: id.cloned(),
            severity: None,
            title: String::new(),
            change,
        }
    }
}

impl From<&FindingEvent> for Happening {
    fn from(event: &FindingEvent) -> Self {
        match event {
            FindingEvent::Posted(p) => Happening::of(&p.finding, Change::Posted),
            FindingEvent::Updated(p) => Happening::of(&p.finding, Change::Updated),
            FindingEvent::Withdrawn(p) => Happening::about(
                Some(&p.id),
                Change::Withdrawn {
                    reason: p.reason.clone(),
                },
            ),
            FindingEvent::Refused(p) => Happening::about(
                p.id.as_ref(),
                Change::Refused {
                    operation: p.operation,
                    problems: p.report.diagnostics.len(),
                },
            ),
            FindingEvent::Answered(p) => Happening::about(
                Some(&p.id),
                Change::Answered {
                    of: p.node.clone(),
                    answer: p.answer,
                    why: p.why.clone(),
                },
            ),
            FindingEvent::Proved(p) => Happening::about(
                Some(&p.id),
                Change::Proved {
                    of: p.node.clone(),
                    cmd: p.result.cmd.clone(),
                    passed: p.result.exit_code == 0,
                },
            ),
            FindingEvent::Settled(p) => {
                Happening::about(Some(&p.id), Change::Settled { of: p.node.clone() })
            }
        }
    }
}
