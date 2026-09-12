//! What a run's findings mean to the run that inherits them.
//!
//! A findings file's own shape and rules are `yunta-core`'s — they are a
//! total function from a document to its problems. What needs the engine
//! is the reading that only a log can answer: which findings a
//! successor starts from.

use std::collections::HashSet;

use yunta_core::events::findings::effective;
use yunta_core::events::{Finding, StoredEvent};

/// The findings a successor inherits, derived purely from the parent's
/// own log: every finding the log leaves standing — the content of its
/// latest posting, and nothing its node took back — deduplicated by
/// location + title normalized for case and whitespace. The first
/// standing occurrence's full record wins, so no authorship or detail is
/// lost to the collapse. Deterministic: same log, same output — the
/// promotion close serializes exactly this into
/// `artifacts/findings-inherited.yaml`.
pub fn inherited_findings(events: &[StoredEvent]) -> Vec<Finding> {
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut inherited = Vec::new();
    for posted in effective(events) {
        let key = (
            posted.finding.location.clone(),
            normalized_title(&posted.finding.title),
        );
        if seen.insert(key) {
            inherited.push(posted.finding);
        }
    }
    inherited
}

/// Case- and whitespace-insensitive: "Scope  expansion DENIED" and
/// "scope expansion denied" are the same complaint about the same place.
fn normalized_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{
        EventBody, EventPayload, Finding, FindingPostedPayload, FindingSeverity,
        FindingUpdatedPayload, FindingWithdrawnPayload, StoredEvent,
    };

    use super::*;

    fn event(seq: u64, node: &str, payload: EventPayload) -> StoredEvent {
        StoredEvent {
            run_id: "run-1".into(),
            seq: seq.into(),
            timestamp: chrono::DateTime::UNIX_EPOCH,
            node_id: Some(node.into()),
            body: EventBody::Known(payload),
        }
    }

    fn finding(id: &str, title: &str, location: &str) -> Finding {
        Finding {
            id: id.into(),
            severity: FindingSeverity::Major,
            title: title.to_string(),
            location: location.to_string(),
            detail: "detail".to_string(),
            proposed_criterion: None,
        }
    }

    #[test]
    fn an_updated_finding_is_inherited_with_its_new_content() {
        let events = vec![
            event(
                1,
                "review",
                EventPayload::FindingPosted(FindingPostedPayload {
                    finding: finding("f1", "Scope expansion denied", "tasks/T001"),
                }),
            ),
            event(
                2,
                "review",
                EventPayload::FindingUpdated(FindingUpdatedPayload {
                    finding: Finding {
                        detail: "the denial was narrowed to one file".to_string(),
                        ..finding("f1", "Scope expansion narrowed", "tasks/T001")
                    },
                }),
            ),
        ];

        let inherited = inherited_findings(&events);
        assert_eq!(inherited.len(), 1, "one id, one inherited finding");
        assert_eq!(inherited[0].title, "Scope expansion narrowed");
        assert_eq!(inherited[0].detail, "the denial was narrowed to one file");
    }

    #[test]
    fn an_updated_title_collapses_duplicates_of_the_title_it_now_carries() {
        let events = vec![
            event(
                1,
                "review",
                EventPayload::FindingPosted(FindingPostedPayload {
                    finding: finding("f1", "Scope expansion denied", "tasks/T001"),
                }),
            ),
            event(
                2,
                "review",
                EventPayload::FindingPosted(FindingPostedPayload {
                    finding: finding("f2", "Missing rollback", "tasks/T001"),
                }),
            ),
            // `f1` now says what `f2` says: they are one complaint about
            // one place, and `f1` was posted first.
            event(
                3,
                "review",
                EventPayload::FindingUpdated(FindingUpdatedPayload {
                    finding: finding("f1", "missing  ROLLBACK", "tasks/T001"),
                }),
            ),
        ];

        let inherited = inherited_findings(&events);
        assert_eq!(inherited.len(), 1, "got: {inherited:?}");
        assert_eq!(inherited[0].id, "f1");
    }

    #[test]
    fn a_withdrawn_finding_is_not_inherited() {
        let events = vec![
            event(
                1,
                "review",
                EventPayload::FindingPosted(FindingPostedPayload {
                    finding: finding("f1", "Scope expansion denied", "tasks/T001"),
                }),
            ),
            event(
                2,
                "review",
                EventPayload::FindingPosted(FindingPostedPayload {
                    finding: finding("f2", "Missing rollback", "tasks/T002"),
                }),
            ),
            event(
                3,
                "review",
                EventPayload::FindingWithdrawn(FindingWithdrawnPayload {
                    id: "f1".into(),
                    reason: "the scope was approved after all".to_string(),
                }),
            ),
        ];

        let inherited = inherited_findings(&events);
        assert_eq!(inherited.len(), 1, "got: {inherited:?}");
        assert_eq!(inherited[0].id, "f2");
    }

    #[test]
    fn a_withdrawal_frees_the_dedup_key_for_the_finding_that_still_stands() {
        let events = vec![
            event(
                1,
                "review",
                EventPayload::FindingPosted(FindingPostedPayload {
                    finding: finding("f1", "Scope expansion denied", "tasks/T001"),
                }),
            ),
            event(
                2,
                "review",
                EventPayload::FindingPosted(FindingPostedPayload {
                    finding: finding("f2", "scope  expansion DENIED", "tasks/T001"),
                }),
            ),
            event(
                3,
                "review",
                EventPayload::FindingWithdrawn(FindingWithdrawnPayload {
                    id: "f1".into(),
                    reason: "posted against the wrong task".to_string(),
                }),
            ),
        ];

        let inherited = inherited_findings(&events);
        assert_eq!(inherited.len(), 1, "got: {inherited:?}");
        assert_eq!(inherited[0].id, "f2");
    }
}
