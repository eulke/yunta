//! The samples of the finding kinds: a finding posted, updated, withdrawn,
//! refused, answered, proved, and settled.

use crate::events::findings::FindingSays;
use crate::events::*;

pub(crate) fn finding_posted() -> FindingPostedPayload {
    FindingPostedPayload {
        finding: Finding {
            id: "f-1".into(),
            severity: FindingSeverity::Major,
            says: FindingSays::Written {
                title: "missing error handling".to_string(),
                detail: "unwrap on a fallible call".to_string(),
            },
            location: "crates/cli/src/main.rs:10".into(),
            proposed_criterion: None,
        },
    }
}

pub(crate) fn finding_updated() -> FindingUpdatedPayload {
    FindingUpdatedPayload {
        finding: Finding {
            id: "f-1".into(),
            severity: FindingSeverity::Blocking,
            says: FindingSays::Written {
                title: "missing error handling".to_string(),
                detail: "unwrap on a fallible call, reached on every run".to_string(),
            },
            location: "crates/cli/src/main.rs:10-14".into(),
            proposed_criterion: None,
        },
    }
}

pub(crate) fn finding_withdrawn() -> FindingWithdrawnPayload {
    FindingWithdrawnPayload {
        id: "f-1".into(),
        reason: "the call it named is gone".to_string(),
    }
}

pub(crate) fn finding_refused() -> FindingRefusedPayload {
    FindingRefusedPayload {
        operation: FindingOperation::Post,
        id: Some("f-3".into()),
        report: crate::diagnostic::Report::new(
            crate::diagnostic::DocumentRef::new(
                crate::ArtifactKind::Findings,
                crate::ArtifactKind::POST_FINDING_TOOL,
            ),
            vec![
                crate::diagnostic::Diagnostic::new(
                    crate::diagnostic::Subject::Document,
                    crate::diagnostic::Problem::parse("severity", "unknown variant `big`"),
                ),
                crate::diagnostic::Diagnostic::new(
                    crate::diagnostic::Subject::Finding(crate::diagnostic::Named::new(
                        crate::FindingId::from("f-3"),
                        0,
                    )),
                    crate::diagnostic::Problem::broke(crate::findings::FindingFault::EmptyDetail),
                ),
            ],
        ),
    }
}

pub(crate) fn finding_answered() -> FindingAnsweredPayload {
    FindingAnsweredPayload {
        node: "only".into(),
        id: "f-1".into(),
        answer: FindingAnswer::Fixed,
        why: "the call returns its error now".to_string(),
    }
}

pub(crate) fn finding_proved() -> FindingProvedPayload {
    FindingProvedPayload {
        node: "only".into(),
        id: "f-1".into(),
        result: CriterionResult {
            cmd: "cargo test -p cli errors".to_string(),
            exit_code: 0,
            r#type: None,
            reused: false,
            duration_ms: Some(120),
            output: None,
            tail: Vec::new(),
            tree: None,
            head: None,
            conclusion: None,
        },
    }
}

pub(crate) fn finding_settled() -> FindingSettledPayload {
    FindingSettledPayload {
        node: Some("only".into()),
        id: "f-1".into(),
        caused_by: 7.into(),
    }
}
