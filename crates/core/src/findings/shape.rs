//! What a findings artifact is, in the vocabulary its own readers use.

use crate::diagnostic::{Named, Subject};
use crate::events::FindingSeverity;
use crate::shape::{Hint, Walk};
use crate::yaml::Value;
use crate::FindingId;

use super::EXAMPLE;

/// The keys a finding declares. Restated from [`crate::Finding`] and
/// held true by the schema test that reads the generated JSON Schema —
/// see `crate::ledger::shape` for why the restatement exists at all.
pub(crate) const FINDING_REQUIRED: &[&str] = &["id", "severity", "title", "location", "detail"];
pub(crate) const FINDING_OPTIONAL: &[&str] = &["proposed_criterion"];

const FINDING_HINTS: &[Hint] = &[
    (
        "description",
        "detail",
        "the body of a finding is `detail:`",
    ),
    ("message", "title", "the one-line summary is `title:`"),
    (
        "file",
        "location",
        "the path and its range go together in `location:`",
    ),
    (
        "line",
        "location",
        "the path and its range go together in `location:`",
    ),
    (
        "path",
        "location",
        "the path and its range go together in `location:`",
    ),
    (
        "severity_level",
        "severity",
        "the ladder is declared with `severity:`",
    ),
];

pub(crate) const PROPOSED_CRITERION_REQUIRED: &[&str] = &["cmd"];

pub(super) fn diagnose(value: &Value, walk: &mut Walk) {
    walk.entries(value, "findings", EXAMPLE, finding);
}

/// One finding, named by its own id when that parsed.
fn finding(walk: &mut Walk, index: usize, item: &Value) {
    walk.at(Subject::Finding(Named::new(None, index)));
    let Some(map) = walk.mapping(item, "a mapping", "- id: null-deref-on-resize") else {
        return;
    };
    let named = Named::new(walk.id::<FindingId>(map), index);
    walk.at(Subject::Finding(named))
        .keys(map, FINDING_REQUIRED, FINDING_OPTIONAL, FINDING_HINTS)
        .string(map, "title", "title: \"Resize handler dereferences null\"")
        .string(map, "location", "location: \"src/ui/resize.rs:142-150\"")
        .string(map, "detail", "detail: \"What goes wrong, and when.\"")
        .one_of(
            map,
            "severity",
            &FindingSeverity::NAMES,
            "severity: blocking",
        );

    if let Some(criterion) = map.get("proposed_criterion") {
        if let Some(inner) = walk.mapping(
            criterion,
            "a mapping",
            "proposed_criterion: { cmd: \"cargo test resize\" }",
        ) {
            walk.keys(inner, PROPOSED_CRITERION_REQUIRED, &[], &[]);
        }
    }
}
