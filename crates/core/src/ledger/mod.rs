//! The task-ledger schema: the types a ledger parses into, the shape
//! published to whoever writes one, and the rules that only hold across
//! the whole document.
//!
//! All three live together because they are one schema. Split across
//! crates, a caller could reach the types without the rules, and one
//! did.

use serde::{Deserialize, Serialize};

use crate::events::{self, CriterionType};
use crate::TaskId;

/// A ledger document — the sole top-level key is `tasks:`, with no
/// header metadata alongside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    pub tasks: Vec<Task>,
}

/// One criterion as the ledger declares it: a command, and whether it
/// is a `guard` (passes before and after the work) or an ordinary
/// criterion (red before, green after). The event log freezes it as
/// [`events::Criterion`], which reads what a later writer adds; this
/// type refuses it, because an agent wrote it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<CriterionType>,
}

impl Criterion {
    /// Whether the criterion is a no-regression guard.
    pub fn is_guard(&self) -> bool {
        self.r#type == Some(CriterionType::Guard)
    }
}

impl From<&Criterion> for events::Criterion {
    fn from(criterion: &Criterion) -> Self {
        events::Criterion {
            cmd: criterion.cmd.clone(),
            r#type: criterion.r#type.clone(),
        }
    }
}

/// One task. `id`'s pattern isn't enforced by this type — the
/// spec treats that as a registration-time rule, not a parse-time
/// one, so an ill-formed id still parses and gets a proper diagnostic
/// naming the task, field and expectation instead of a raw serde error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub scope: Vec<String>,
    pub criteria: Vec<Criterion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub manual_review: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justification: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !b
}

mod rules;
pub(crate) mod shape;

/// The shape this document publishes, as the YAML it is.
///
/// It lives as a file rather than a string literal, so an editor reads
/// it as YAML and a person reviewing a schema change sees the diff in
/// the format the change is about. `include_str!` binds it at compile
/// time, and the test that reads it back through
/// [`read`](crate::shape::read) is what stops it drifting from the
/// parser.
const EXAMPLE: &str = include_str!("shape.yaml");

impl crate::shape::Document for Ledger {
    const KIND: crate::ArtifactKind = crate::ArtifactKind::TaskLedger;
    const EXAMPLE: &'static str = EXAMPLE;

    fn diagnose(value: &crate::yaml::Value, walk: &mut crate::shape::Walk) {
        shape::diagnose(value, walk);
    }

    fn check(&self) -> Vec<crate::diagnostic::Diagnostic> {
        rules::check(self)
    }
}
