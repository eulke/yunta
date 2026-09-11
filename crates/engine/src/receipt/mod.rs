//! Verified Work Receipt: a PR-attachable
//! certificate derived **entirely** from the event log — never a summary
//! an agent wrote. "El recibo ES la evidencia": every number here traces
//! back to a specific event kind, the same discipline [`crate::stats`]
//! and [`crate::progress`] already hold to. No new bookkeeping — this
//! module only reads what the engine already recorded. Turning it
//! into something a person or a program reads is [`render`]'s job.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use serde::Serialize;

use yunta_core::diagnostic::ArtifactFailure;
use yunta_core::events::{
    CheckVerdict, EventPayload, Failure, Phase, StoredEvent, TerminalState, TokenUsage,
};
use yunta_core::ContentHash;
use yunta_core::{
    AdapterId, ArtifactKind, CheckBuiltin, Manifest, ModeName, ModelName, NodeId, NodeKind, RunId,
    RunnerName, Seq,
};

use crate::replay::{derive, NodeState};
use crate::replay::{unknown_kind_counts, UnknownKindCount};

mod render;

pub use render::{fan_out_groups, render_json, render_markdown};

#[derive(Debug, thiserror::Error)]
pub enum ReceiptError {
    /// A receipt certifies *closed* work — a run still
    /// `running`/`waiting`/`paused` has no
    /// `run_finished` metrics (CPTV, final token total) to report yet.
    #[error(
        "run `{0}` hasn't reached a terminal state yet — `yunta status {0}` shows where it is; \
         a receipt is only generated once a run finishes"
    )]
    NotFinished(RunId),
}

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
    /// `baseline_compare` nodes that actually compared against the
    /// capture (the run's first `baseline_compare` only captures — it
    /// has nothing yet to regress against).
    pub compared: usize,
    pub regressions: usize,
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

/// The receipt's own reading of [`yunta_storage::ChainVerification`] —
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
    pub run_id: RunId,
    pub workflow: String,
    pub mode: ModeName,
    pub terminal_state: TerminalState,
    pub criteria: CriteriaSummary,
    pub baseline: Option<BaselineSummary>,
    pub scope: ScopeSummary,
    pub runners: Vec<RunnerUsage>,
    pub cost: CostSummary,
    pub event_chain: EventChainStatus,
    /// Events this binary could not interpret, by kind — a run with any
    /// is certified only for what the binary understood.
    pub unknown_kinds: Vec<UnknownKindCount>,
    /// What the run's documents got wrong, counted by the document kind
    /// the rule was asked of and the stable name of each kind of
    /// problem.
    ///
    /// Counting is the whole reason a diagnostic is a value: a receipt
    /// that had to read prose could only reprint it, and "how often does
    /// a ledger come back unreadable" is a question nobody can answer by
    /// grepping free text. The kind is half the answer — `duplicate-id`
    /// is one rule asked of three documents, so three broken ledgers and
    /// one of each are different facts and count separately.
    pub diagnostics: Vec<DiagnosticCount>,
    /// Which layer the run's document problems were caught in.
    ///
    /// The counts above only ever see what reached a node's close. A
    /// session that checked its own artifact and fixed it leaves no
    /// failure behind, so without this the run reads as if nothing had
    /// gone wrong — and "was the writer told enough, or did it find out
    /// by failing?" stays unanswerable.
    pub self_checks: SelfCheckSummary,
}

/// What the run's sessions asked of `yunta_check_artifact`, and what it
/// answered.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct SelfCheckSummary {
    /// Checks the sessions ran.
    pub checks: usize,
    /// Of those, how many answered clean.
    pub clean: usize,
    /// Nodes that declared an interpreted artifact and never checked one.
    /// A failure from one of these is a failure nothing tried to prevent.
    pub nodes_that_never_checked: Vec<NodeId>,
    /// What the sessions corrected in place, by code — the problems this
    /// run survived without spending a repair on them.
    pub corrected: Vec<DiagnosticCount>,
}

/// How many times a run hit one kind of problem in one kind of document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiagnosticCount {
    /// The document whose rules were asked. `None` for a problem with
    /// the file itself — a file that was never written has no content
    /// to have a kind.
    pub kind: Option<ArtifactKind>,
    pub code: String,
    pub occurrences: usize,
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

/// Every problem the log recorded, counted by `(document kind, code)`,
/// most frequent first and ties broken by name so the same log always
/// renders the same receipt.
/// Which layer caught what: every check a session ran, and every node
/// that could have run one and did not.
fn self_checks(events: &[StoredEvent], manifest: &Manifest) -> SelfCheckSummary {
    let mut summary = SelfCheckSummary::default();
    let mut corrected: HashMap<(Option<ArtifactKind>, String), usize> = HashMap::new();
    let mut checked: HashSet<&NodeId> = HashSet::new();

    for event in events {
        let Some(EventPayload::ArtifactChecked(p)) = event.payload() else {
            continue;
        };
        summary.checks += 1;
        if let Some(node) = &event.node_id {
            checked.insert(node);
        }
        match &p.verdict {
            CheckVerdict::Ok => summary.clean += 1,
            CheckVerdict::Problems { codes } => {
                for code in codes {
                    *corrected
                        .entry((p.artifact_kind, code.clone()))
                        .or_default() += 1;
                }
            }
        }
    }

    // A node with nothing interpreted to check is not a node that skipped
    // one: the tool has nothing to offer it.
    summary.nodes_that_never_checked = manifest
        .workflow
        .nodes
        .iter()
        .filter(|node| {
            node.artifacts
                .as_ref()
                .is_some_and(|a| a.produces.iter().any(|spec| spec.kind().is_some()))
        })
        .map(|node| &node.id)
        .filter(|id| !checked.contains(id))
        .cloned()
        .collect();

    summary.corrected = sorted_counts(corrected);
    summary
}

fn diagnostic_counts(events: &[StoredEvent]) -> Vec<DiagnosticCount> {
    let mut counts: HashMap<(Option<ArtifactKind>, &'static str), usize> = HashMap::new();
    for event in events {
        let Some(EventPayload::NodeFailed(p)) = event.payload() else {
            continue;
        };
        let Failure::Artifacts { artifacts } = &p.failure else {
            continue;
        };
        for failure in artifacts {
            match failure {
                // A problem with the file itself is counted under its own
                // code and no kind: `artifact-missing` is the same fact
                // whatever the file was going to contain.
                ArtifactFailure::File { problem, .. } => {
                    *counts.entry((None, problem.code())).or_default() += 1;
                }
                ArtifactFailure::Content(report) => {
                    for diagnostic in &report.diagnostics {
                        *counts
                            .entry((Some(report.document.kind), diagnostic.code()))
                            .or_default() += 1;
                    }
                }
            }
        }
    }
    sorted_counts(counts)
}

/// Counts in the order a reader wants them: what happened most, then by
/// name so a receipt of the same run is byte-identical twice.
fn sorted_counts<C: Into<String>>(
    counts: HashMap<(Option<ArtifactKind>, C), usize>,
) -> Vec<DiagnosticCount> {
    let mut counts: Vec<DiagnosticCount> = counts
        .into_iter()
        .map(|((kind, code), occurrences)| DiagnosticCount {
            kind,
            code: code.into(),
            occurrences,
        })
        .collect();
    counts.sort_by(|a, b| {
        b.occurrences
            .cmp(&a.occurrences)
            .then_with(|| a.code.cmp(&b.code))
            .then_with(|| {
                a.kind
                    .map(ArtifactKind::as_str)
                    .cmp(&b.kind.map(ArtifactKind::as_str))
            })
    });
    counts
}

/// Builds a [`Receipt`] purely from `manifest` + `events` (+ the chain
/// status the caller already computed via `Storage::verify_chain`, since
/// that alone needs IO). Pure otherwise — same log, same receipt, always.
pub fn build_receipt(
    run_id: &RunId,
    manifest: &Manifest,
    events: &[StoredEvent],
    event_chain: EventChainStatus,
) -> Result<Receipt, ReceiptError> {
    let Some((terminal_state, metrics)) = events.iter().find_map(|e| match e.payload() {
        Some(EventPayload::RunFinished(p)) => Some((p.terminal_state, p.metrics.clone())),
        _ => None,
    }) else {
        return Err(ReceiptError::NotFinished(run_id.clone()));
    };

    let criteria = criteria_summary(events);
    let baseline = baseline_summary(manifest, events);
    let scope = scope_summary(events);
    let runners = runner_usage(events);
    let unknown_kinds = unknown_kind_counts(&crate::replay::derive(events));
    let reroutes = events
        .iter()
        .filter(|e| matches!(e.payload(), Some(EventPayload::NodeRerouted(_))))
        .count();

    Ok(Receipt {
        run_id: run_id.clone(),
        workflow: manifest.workflow.name.clone(),
        mode: yunta_core::events::run_mode(events),
        terminal_state,
        criteria,
        baseline,
        scope,
        runners,
        cost: CostSummary {
            tokens: metrics.tokens,
            cptv: metrics.cptv,
            reroutes,
        },
        event_chain,
        unknown_kinds,
        diagnostics: diagnostic_counts(events),
        self_checks: self_checks(events, manifest),
    })
}

/// The latest post-check `criteria_checked` per task — a retried task's
/// earlier, superseded attempts don't get counted twice: the log keeps
/// every attempt, but the receipt certifies only the final verdict.
fn criteria_summary(events: &[StoredEvent]) -> CriteriaSummary {
    let mut latest_post: HashMap<String, &yunta_core::events::CriteriaCheckedPayload> =
        HashMap::new();
    for event in events {
        if let Some(EventPayload::CriteriaChecked(p)) = event.payload() {
            if p.phase == Phase::Post {
                latest_post.insert(p.task_id.to_string(), p);
            }
        }
    }
    let mut entries: Vec<CriterionEntry> = latest_post
        .values()
        .flat_map(|p| {
            p.results.iter().map(|r| CriterionEntry {
                task_id: p.task_id.to_string(),
                cmd: r.cmd.clone(),
                exit_code: r.exit_code,
            })
        })
        .collect();
    entries.sort_by(|a, b| (&a.task_id, &a.cmd).cmp(&(&b.task_id, &b.cmd)));
    let green = entries.iter().filter(|e| e.exit_code == 0).count();
    CriteriaSummary {
        total: entries.len(),
        green,
        entries,
    }
}

fn baseline_summary(manifest: &Manifest, events: &[StoredEvent]) -> Option<BaselineSummary> {
    // The one node that emitted `baseline_captured` did the capturing;
    // every other `baseline_compare` node compared. Read from the event
    // kind and the envelope's node id, never the node's outcome text.
    let capturing_node = events.iter().find_map(|e| match e.payload() {
        Some(EventPayload::BaselineCaptured(_)) => e.node_id.clone(),
        _ => None,
    });
    let captured = events.iter().find_map(|e| match e.payload() {
        Some(EventPayload::BaselineCaptured(p)) => Some(p),
        _ => None,
    })?;

    let state = derive(events);
    let mut compared = 0usize;
    let mut regressions = 0usize;
    for node in manifest.workflow.iter_nodes() {
        if !matches!(&node.kind, NodeKind::Check(CheckBuiltin::BaselineCompare)) {
            continue;
        }
        // The run's very first `baseline_compare` only captures — it has
        // nothing yet to compare against, so it isn't counted.
        if capturing_node.as_ref() == Some(&node.id) {
            continue;
        }
        match state.nodes.get(&node.id) {
            Some(NodeState::Finished { .. }) => compared += 1,
            Some(NodeState::Failed { .. }) => {
                compared += 1;
                regressions += 1;
            }
            _ => {}
        }
    }

    Some(BaselineSummary {
        suite: captured.command.clone(),
        hash: captured.hash.clone(),
        compared,
        regressions,
    })
}

fn scope_summary(events: &[StoredEvent]) -> ScopeSummary {
    let mut files: BTreeSet<PathBuf> = BTreeSet::new();
    let mut violations: BTreeSet<PathBuf> = BTreeSet::new();
    for event in events {
        if let Some(EventPayload::ScopeChecked(p)) = event.payload() {
            files.extend(p.diff.iter().cloned());
            violations.extend(p.violations.iter().cloned());
        }
    }
    ScopeSummary {
        files_touched: files.len(),
        violations: violations.into_iter().collect(),
    }
}

/// Which runner ran each node, in the order the nodes first resolved
/// one.
///
/// A node resolves its runner once per session it opens — again for a
/// repair, again after a re-route — and the receipt's line counts
/// runners, not resolutions, so each node appears once. Fan-out siblings
/// carry distinct ids (`<base>@<runner>`), so they are not collapsed by
/// this.
fn runner_usage(events: &[StoredEvent]) -> Vec<RunnerUsage> {
    let mut seen: BTreeSet<NodeId> = BTreeSet::new();
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::RunnerResolved(p)) => {
                let node_id = e.node_id.clone()?;
                seen.insert(node_id.clone()).then(|| RunnerUsage {
                    node_id,
                    runner: p.runner.clone(),
                    adapter: p.chosen.adapter.clone(),
                    model: p.chosen.model.clone(),
                })
            }
            _ => None,
        })
        .collect()
}
