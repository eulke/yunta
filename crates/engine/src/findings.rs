//! What a run's findings mean to the run that inherits them.
//!
//! A findings file's own shape and rules are `yunta-core`'s — they are a
//! total function from a document to its problems. What needs the engine
//! is the reading that only a log can answer: which findings a
//! successor starts from.

use yunta_core::events::findings::PostedFinding;
use yunta_core::events::{Finding, StoredEvent};
use yunta_core::FindingId;

/// The findings a successor inherits, derived purely from the parent's
/// own log: every finding the log leaves standing and nothing settled —
/// the content of its latest posting, nothing its node took back, and
/// nothing a proof or a person settled — each under its node's name
/// before its id, deduplicated by location and by the complaint each
/// finding makes ([`Finding::complaint`]): a session's title compared
/// case- and whitespace-insensitively, or the cause an engine finding
/// names. The first standing occurrence's full record wins, so no
/// authorship or detail is lost to the collapse. Deterministic: same log,
/// same output — the promotion close accepts exactly this as the run's own
/// findings artifact.
pub fn inherited_findings(events: &[StoredEvent]) -> Vec<Finding> {
    let standing: Vec<Finding> = yunta_core::events::findings::FindingLedger::of(events)
        .unsettled()
        .into_iter()
        .map(qualified)
        .collect();
    crate::replay::dedup_findings(&standing)
}

/// `posted`'s finding under an id no other node's finding shares: the
/// node that reported it before its own id — ids are unique only within
/// one node, and the successor holds them all as the run's. The engine's
/// own findings keep theirs.
fn qualified(posted: PostedFinding) -> Finding {
    let Some(node) = posted.node else {
        return posted.finding;
    };
    let id = FindingId::try_from(format!("{node}/{}", posted.finding.id))
        .unwrap_or_else(|_| posted.finding.id.clone());
    Finding {
        id,
        ..posted.finding
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{
        AskedScope, EventPayload, Finding, FindingCause, FindingPostedPayload, FindingSays,
        FindingSeverity, FindingUpdatedPayload, FindingWithdrawnPayload, ScopeAsker,
    };
    use yunta_core::expansion::Denial;
    use yunta_testkit_core::Log;

    use super::*;
    use yunta_core::events::FindingEvent;

    fn finding(id: &str, title: &str, location: &str) -> Finding {
        Finding {
            id: id.into(),
            severity: FindingSeverity::Major,
            says: FindingSays::Written {
                title: title.to_string(),
                detail: "detail".to_string(),
            },
            location: location.into(),
            proposed_criterion: None,
        }
    }

    #[test]
    fn a_settled_finding_is_not_inherited() {
        let events = Log::for_run("run-1")
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                    finding: finding("f1", "Missing file", "fixed.txt"),
                })),
            )
            .node(
                "fix",
                EventPayload::Findings(FindingEvent::Proved(
                    yunta_core::events::FindingProvedPayload {
                        node: "review".into(),
                        id: "f1".into(),
                        result: yunta_core::events::CriterionResult {
                            cmd: "test -f fixed.txt".to_string(),
                            exit_code: 0,
                            r#type: None,
                            reused: false,
                            duration_ms: None,
                            output: None,
                            tail: Vec::new(),
                            tree: None,
                            head: None,
                            conclusion: None,
                        },
                    },
                )),
            )
            .build();

        assert!(inherited_findings(&events).is_empty());
    }

    #[test]
    fn an_updated_finding_is_inherited_with_its_new_content() {
        let events = Log::for_run("run-1")
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                    finding: finding("f1", "Scope expansion denied", "tasks/T001"),
                })),
            )
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Updated(FindingUpdatedPayload {
                    finding: Finding {
                        says: FindingSays::Written {
                            title: "Scope expansion narrowed".to_string(),
                            detail: "the denial was narrowed to one file".to_string(),
                        },
                        ..finding("f1", "Scope expansion narrowed", "tasks/T001")
                    },
                })),
            )
            .build();

        let inherited = inherited_findings(&events);
        assert_eq!(inherited.len(), 1, "one id, one inherited finding");
        assert_eq!(
            inherited[0].says,
            FindingSays::Written {
                title: "Scope expansion narrowed".to_string(),
                detail: "the denial was narrowed to one file".to_string(),
            }
        );
    }

    #[test]
    fn an_updated_title_collapses_duplicates_of_the_title_it_now_carries() {
        let events = Log::for_run("run-1")
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                    finding: finding("f1", "Scope expansion denied", "tasks/T001"),
                })),
            )
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                    finding: finding("f2", "Missing rollback", "tasks/T001"),
                })),
            )
            // `f1` now says what `f2` says: they are one complaint about
            // one place, and `f1` was posted first.
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Updated(FindingUpdatedPayload {
                    finding: finding("f1", "missing  ROLLBACK", "tasks/T001"),
                })),
            )
            .build();

        let inherited = inherited_findings(&events);
        assert_eq!(inherited.len(), 1, "got: {inherited:?}");
        assert_eq!(inherited[0].id, "review/f1");
    }

    #[test]
    fn a_withdrawn_finding_is_not_inherited() {
        let events = Log::for_run("run-1")
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                    finding: finding("f1", "Scope expansion denied", "tasks/T001"),
                })),
            )
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                    finding: finding("f2", "Missing rollback", "tasks/T002"),
                })),
            )
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Withdrawn(FindingWithdrawnPayload {
                    id: "f1".into(),
                    reason: "the scope was approved after all".to_string(),
                })),
            )
            .build();

        let inherited = inherited_findings(&events);
        assert_eq!(inherited.len(), 1, "got: {inherited:?}");
        assert_eq!(inherited[0].id, "review/f2");
    }

    #[test]
    fn a_withdrawal_frees_the_dedup_key_for_the_finding_that_still_stands() {
        let events = Log::for_run("run-1")
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                    finding: finding("f1", "Scope expansion denied", "tasks/T001"),
                })),
            )
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                    finding: finding("f2", "scope  expansion DENIED", "tasks/T001"),
                })),
            )
            .node(
                "review",
                EventPayload::Findings(FindingEvent::Withdrawn(FindingWithdrawnPayload {
                    id: "f1".into(),
                    reason: "posted against the wrong task".to_string(),
                })),
            )
            .build();

        let inherited = inherited_findings(&events);
        assert_eq!(inherited.len(), 1, "got: {inherited:?}");
        assert_eq!(inherited[0].id, "review/f2");
    }

    /// Refusals of one task are one complaint whatever the case of the
    /// task's name, and a refusal of another task is another: four refusals
    /// at one place leave two findings, the first refusal of each complaint.
    #[test]
    fn repeated_refusals_of_one_task_are_one_complaint() {
        let refused = |id: &str, task: &str, reason: &str| {
            EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                finding: Finding {
                    id: id.into(),
                    severity: FindingSeverity::Minor,
                    says: FindingSays::Found {
                        cause: FindingCause::ScopeDenied {
                            asker: ScopeAsker::Task(task.into()),
                            denial: Denial::ModeDeny,
                            asked: Some(AskedScope {
                                paths: vec!["b.txt".into()],
                                reason: Some(reason.to_string()),
                            }),
                        },
                    },
                    location: "tasks/T001".into(),
                    proposed_criterion: None,
                },
            }))
        };
        let events = Log::for_run("run-1")
            .event(refused("scope-expansion-T001-1", "T001", "first"))
            .event(refused("scope-expansion-T001-2", "T001", "second"))
            .event(refused("scope-expansion-t001-3", "t001", "third"))
            .event(refused("scope-expansion-T002-1", "T002", "fourth"))
            .build();

        let inherited = inherited_findings(&events);
        let ids: Vec<&str> = inherited.iter().map(|found| found.id.as_str()).collect();
        assert_eq!(ids, ["scope-expansion-T001-1", "scope-expansion-T002-1"]);
    }
}
