//! The findings artifact's registration rules.
//!
//! Shape is `yunta-core`'s frontier: by the time a `FindingsFile`
//! exists, every key is known and every field is present. What is left
//! is the one rule that spans the whole document — an id used twice —
//! and the emptiness a `String` cannot refuse on its own.

use std::collections::HashSet;

use yunta_core::diagnostic::{Diagnostic, Problem, Subject};
use yunta_core::{FindingId, FindingsFile};

fn broke(index: usize, id: &FindingId, code: &'static str, detail: &str) -> Diagnostic {
    Diagnostic::new(
        Subject::Finding {
            id: Some(id.clone()),
            index,
        },
        Problem::rule(code, detail),
    )
}

/// Validates a parsed findings file, collecting every violation rather
/// than stopping at the first.
pub fn register(file: &FindingsFile) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    let mut known_ids: HashSet<&FindingId> = HashSet::new();

    for (index, finding) in file.findings.iter().enumerate() {
        if !known_ids.insert(&finding.id) {
            errors.push(broke(
                index,
                &finding.id,
                "duplicate-id",
                "a second finding already carries this id; every id is declared once",
            ));
        }
        for (value, key) in [
            (&finding.title, "title"),
            (&finding.location, "location"),
            (&finding.detail, "detail"),
        ] {
            if value.trim().is_empty() {
                errors.push(broke(
                    index,
                    &finding.id,
                    match key {
                        "title" => "empty-title",
                        "location" => "empty-location",
                        _ => "empty-detail",
                    },
                    &format!("`{key}` is empty"),
                ));
            }
        }
    }

    errors
}

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
