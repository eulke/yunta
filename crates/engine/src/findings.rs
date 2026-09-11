//! What a run's findings mean to the run that inherits them.
//!
//! A findings file's own shape and rules are `yunta-core`'s — they are a
//! total function from a document to its problems. What needs the engine
//! is the reading that only a log can answer: which findings a
//! successor starts from.

use std::collections::HashSet;

/// The findings a successor inherits, derived purely from
/// the parent's own log — every `finding_posted`, deduplicated by
/// location + title normalized for case and whitespace.
/// The first occurrence's full record wins, so no authorship or detail
/// is lost to the collapse. Deterministic: same log, same output — the
/// promotion close serializes exactly this into
/// `artifacts/findings-inherited.yaml`.
pub fn inherited_findings(
    events: &[yunta_core::events::StoredEvent],
) -> Vec<yunta_core::events::Finding> {
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut inherited = Vec::new();
    for event in events {
        let Some(yunta_core::events::EventPayload::FindingPosted(p)) = event.payload() else {
            continue;
        };
        let key = (
            p.finding.location.clone(),
            normalized_title(&p.finding.title),
        );
        if seen.insert(key) {
            inherited.push(p.finding.clone());
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
