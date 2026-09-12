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
    Rule {
        code: RuleCode::UnknownId,
        demand: "an update or a withdrawal names an id this node posted",
    },
    Rule {
        code: RuleCode::WithdrawnId,
        demand: "a withdrawn id is final: it is not posted, updated or withdrawn again",
    },
    Rule {
        code: RuleCode::EmptyReason,
        demand: "a withdrawal says why, in a non-empty `reason`",
    },
];

/// What a withdrawal owes.
///
/// The three rules above it — the id is one this node posted, and not one
/// it already withdrew — need the run's own log to decide, so they are
/// checked where the log is (the run tool) and published from here, with
/// the rest of the document's demands.
pub(super) fn check_withdrawal(withdrawal: &crate::findings::Withdrawal) -> Vec<Diagnostic> {
    if withdrawal.reason.trim().is_empty() {
        return vec![Diagnostic::new(
            Subject::Finding(Named::new(withdrawal.id.clone(), 0)),
            Problem::rule(
                RuleCode::EmptyReason,
                "say why it no longer stands, so the log keeps the reason",
            ),
        )];
    }
    Vec::new()
}

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
