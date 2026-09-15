//! What a finding event says happened, read as a person reads it.

use crate::events::{FindingEvent, FindingOperation, FindingSeverity};
use crate::FindingId;

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
}

impl From<&FindingEvent> for Happening {
    fn from(event: &FindingEvent) -> Self {
        match event {
            FindingEvent::Posted(p) => Happening::Finding {
                id: Some(p.finding.id.clone()),
                severity: Some(p.finding.severity),
                title: p.finding.title.clone(),
                change: Change::Posted,
            },
            FindingEvent::Updated(p) => Happening::Finding {
                id: Some(p.finding.id.clone()),
                severity: Some(p.finding.severity),
                title: p.finding.title.clone(),
                change: Change::Updated,
            },
            FindingEvent::Withdrawn(p) => Happening::Finding {
                id: Some(p.id.clone()),
                severity: None,
                title: String::new(),
                change: Change::Withdrawn {
                    reason: p.reason.clone(),
                },
            },
            FindingEvent::Refused(p) => Happening::Finding {
                id: p.id.clone(),
                severity: None,
                title: String::new(),
                change: Change::Refused {
                    operation: p.operation,
                    problems: p.report.diagnostics.len(),
                },
            },
        }
    }
}
