//! The task-ledger schema (T1.0 → `docs/spec-ledger.md`) — parsed once at
//! the frontier into these types; validation against the spec's seven
//! registration rules is T5.1, in `yunta-engine` (mirrors `workflow.rs`
//! types living here while `check()` lives in the engine).

use serde::{Deserialize, Serialize};

use crate::events::Criterion;
use crate::TaskId;

/// A ledger document — the sole top-level key is `tasks:` (spec §1: "sin
/// metadatos de cabecera").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    pub tasks: Vec<Task>,
}

/// One task (spec §2). `id`'s pattern isn't enforced by this type — the
/// spec treats that as a registration-time rule (§3.1), not a parse-time
/// one, so an ill-formed id still parses and gets a proper diagnostic
/// naming the task, field and expectation instead of a raw serde error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
