//! The rules a findings artifact has to satisfy once it is readable.
//!
//! Shape is the frontier before this one: by the time a
//! [`FindingsFile`] exists, every key is known and every field is
//! present. What is left is the one rule that spans the whole document
//! — an id used twice — and the emptiness a `String` cannot refuse on
//! its own.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Named, Problem, Rule, RuleCode, Subject};
use crate::{FindingId, FindingsFile};

/// Every rule this document is held to — see `crate::ledger::rules` for what
/// this list is for and what holds it true.
pub(super) const RULES: &[Rule] = &[
    Rule {
        code: RuleCode::DuplicateId,
        demand: "each `id` is declared once in the file",
    },
    Rule {
        code: RuleCode::EmptyTitle,
        demand: "`title` is a non-empty one-line summary",
    },
    Rule {
        code: RuleCode::EmptyLocation,
        demand: "`location` names a non-empty path, with its range when there is one",
    },
    Rule {
        code: RuleCode::EmptyDetail,
        demand: "`detail` is non-empty: what goes wrong, and when",
    },
];

fn broke(index: usize, id: &FindingId, code: RuleCode, detail: &str) -> Diagnostic {
    Diagnostic::new(
        Subject::Finding(Named::new(id.clone(), index)),
        Problem::rule(code, detail),
    )
}

/// Every violation the file carries, collected rather than stopped at
/// the first.
pub(super) fn check(file: &FindingsFile) -> Vec<Diagnostic> {
    let mut broken = Vec::new();
    let mut known_ids: HashSet<&FindingId> = HashSet::new();

    for (index, finding) in file.findings.iter().enumerate() {
        if !known_ids.insert(&finding.id) {
            broken.push(broke(
                index,
                &finding.id,
                RuleCode::DuplicateId,
                "a second finding already carries this id; every id is declared once",
            ));
        }
        for (value, key, code) in [
            (&finding.title, "title", RuleCode::EmptyTitle),
            (&finding.location, "location", RuleCode::EmptyLocation),
            (&finding.detail, "detail", RuleCode::EmptyDetail),
        ] {
            if value.trim().is_empty() {
                broken.push(broke(
                    index,
                    &finding.id,
                    code,
                    &format!("`{key}` is empty"),
                ));
            }
        }
    }

    broken
}
