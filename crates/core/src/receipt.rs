//! The Verified Work Receipt, as data: what a run held its work to and
//! what it found, every number read off the run's own log. The engine
//! derives it; whoever shows it to a person draws it.

use std::path::PathBuf;

use serde::Serialize;

use crate::diagnostic::{DiagnosticCode, DocumentKind};
use crate::events::{BaselineOrigin, Failure, TerminalState, TokenUsage, UnknownKindCount};
use crate::{
    AdapterId, ContentHash, ModeName, ModelName, NodeId, RunId, RunnerName, Seq, WorkflowName,
};

/// One criterion's final (post-check) verdict — the unit "23/23 criteria
/// green" counts.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CriterionEntry {
    pub task_id: String,
    pub cmd: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CriteriaSummary {
    pub total: usize,
    pub green: usize,
    pub entries: Vec<CriterionEntry>,
}

/// `None` on a [`Receipt`] when the workflow declares no `baseline_compare`
/// check at all — never a manufactured "0 regressions" for a run that
/// never looked; nothing gets invented for what the run never measured.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BaselineSummary {
    pub suite: String,
    pub hash: ContentHash,
    /// `baseline_compare` nodes that ran against the measurement this
    /// run holds.
    pub compared: usize,
    pub regressions: usize,
    /// Who took the measurement: this run, or the root of the lineage it
    /// was born into.
    pub origin: BaselineOrigin,
    /// The code the suite exited with when it was measured, when it was
    /// already failing then: no comparison against it could find a
    /// regression, so its count of none certifies nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub red: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScopeSummary {
    pub files_touched: usize,
    pub violations: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunnerUsage {
    pub node_id: NodeId,
    pub runner: RunnerName,
    pub adapter: AdapterId,
    pub model: ModelName,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostSummary {
    pub tokens: TokenUsage,
    /// Mirrors `crate::stats`'s own CPTV: `None` means no task ever
    /// reached `done` in this run, not a manufactured `0.0`.
    pub cptv: Option<f64>,
    pub reroutes: usize,
}

/// The receipt's own reading of a storage's chain verification —
/// redeclared here rather than depended on directly, so this module's
/// data model stays serializable and storage-agnostic; the CLI command
/// that calls `verify_chain` maps into this.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EventChainStatus {
    Intact { events: usize },
    Broken { seq: Seq, detail: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Receipt {
    /// Version of this document's own schema.
    pub schema_version: u32,
    pub run_id: RunId,
    pub workflow: WorkflowName,
    pub mode: ModeName,
    /// How the run closed; none while it is still open — a receipt a
    /// pull request carries is read as the run's last step begins.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_state: Option<TerminalState>,
    pub criteria: CriteriaSummary,
    pub baseline: Option<BaselineSummary>,
    pub scope: ScopeSummary,
    pub runners: Vec<RunnerUsage>,
    pub cost: CostSummary,
    pub event_chain: EventChainStatus,
    /// Events this binary could not interpret, by kind — a run with any
    /// is certified only for what the binary understood.
    pub unknown_kinds: Vec<UnknownKindCount>,
    /// What the run's declared artifacts got wrong, counted by the
    /// document kind the rule was asked of — `None` where nothing read a
    /// document — and the stable name of each kind of problem.
    ///
    /// Counting is the whole reason a diagnostic is a value: a receipt
    /// that had to read prose could only reprint it, and "how often does
    /// a tasks document come back unreadable" is a question nobody can answer by
    /// grepping free text. The kind is half the answer — `duplicate-id`
    /// is one rule asked of three documents, so three broken tasks documents and
    /// one of each are different facts and count separately.
    pub diagnostics: Vec<DiagnosticCount>,
    /// Each node that stands failed, in the order the workflow declares
    /// them, with why — the question a person brings to a run that did
    /// not finish. Absent while nothing stands failed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<FailedNode>,
}

/// A node that stands failed, and why, as the log recorded it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FailedNode {
    pub node: NodeId,
    pub failure: Failure,
}

/// One kind of problem in one kind of document, and how many times the
/// run hit it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiagnosticCount {
    /// The document whose rules were asked. `None` for a problem with
    /// the artifact itself — a file that was never written, and an
    /// artifact another run owes, have no content to have a kind.
    pub kind: Option<DocumentKind>,
    pub code: DiagnosticCode,
    pub occurrences: usize,
}

impl Receipt {
    /// Version of the receipt's own schema, stamped on every one this
    /// binary writes.
    ///
    /// Not a [`Persisted`](crate::persisted::Persisted) document:
    /// nothing reads a receipt back — `yunta receipt` derives it from
    /// the log every time — so it has no tolerant reader to be. The
    /// version is for whoever consumes `receipt.json` outside yunta,
    /// who has no log to derive it from and needs to know what shape
    /// they were handed.
    pub const SCHEMA_VERSION: u32 = 1;
}

impl std::fmt::Display for DiagnosticCount {
    /// How the count names itself wherever it is read: the code, and
    /// the document it was asked of when there is one.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            Some(kind) => write!(
                f,
                "`{}` in the {} ×{}",
                self.code,
                kind.label(),
                self.occurrences
            ),
            None => write!(f, "`{}` ×{}", self.code, self.occurrences),
        }
    }
}
