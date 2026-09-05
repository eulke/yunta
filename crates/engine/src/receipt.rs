//! Verified Work Receipt: a PR-attachable
//! certificate derived **entirely** from the event log — never a summary
//! an agent wrote. "El recibo ES la evidencia": every number here traces
//! back to a specific event kind, the same discipline [`crate::stats`]
//! and [`crate::progress`] already hold to. No new bookkeeping — this
//! module only reads what the engine already recorded and formats it.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use serde::Serialize;

use yunta_core::events::{EventPayload, Phase, StoredEvent, TerminalState, TokenUsage};
use yunta_core::ContentHash;
use yunta_core::{
    AdapterId, CheckBuiltin, Manifest, ModeName, ModelName, NodeId, NodeKind, RunId, RunnerName,
    Seq,
};

use crate::replay::{derive, NodeState};
use crate::replay::{unknown_kind_counts, UnknownKindCount};

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

fn runner_usage(events: &[StoredEvent]) -> Vec<RunnerUsage> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::RunnerResolved(p)) => Some(RunnerUsage {
                node_id: e.node_id.clone()?,
                runner: p.runner.clone(),
                adapter: p.chosen.adapter.clone(),
                model: p.chosen.model.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// Fan-out siblings share their base id (`<base>@<runner>`, see
/// `manifest.rs`) — grouped here purely for the markdown's "reviewed by
/// N independent runners" line; the JSON receipt exposes the flat
/// `runners` list instead and leaves grouping to whoever consumes it.
pub fn fan_out_groups(runners: &[RunnerUsage]) -> Vec<(NodeId, Vec<&RunnerUsage>)> {
    let mut groups: Vec<(NodeId, Vec<&RunnerUsage>)> = Vec::new();
    for usage in runners {
        if !usage.node_id.is_fan_out() {
            continue;
        }
        let base = usage.node_id.base();
        match groups.iter_mut().find(|(b, _)| *b == base) {
            Some((_, members)) => members.push(usage),
            None => groups.push((base, vec![usage])),
        }
    }
    groups.retain(|(_, members)| members.len() > 1);
    groups
}

/// Markdown rendering — the PR-facing markdown format.
pub fn render_markdown(receipt: &Receipt) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Verified Work Receipt — run {}\n\n",
        receipt.run_id
    ));
    out.push_str(&format!(
        "workflow: `{}` · mode: `{}` · state: {:?}\n\n",
        receipt.workflow, receipt.mode, receipt.terminal_state
    ));

    out.push_str(&format!(
        "- {} {}/{} criteria green (commands + exit codes below)\n",
        mark(receipt.criteria.green == receipt.criteria.total && receipt.criteria.total > 0),
        receipt.criteria.green,
        receipt.criteria.total
    ));
    if !receipt.unknown_kinds.is_empty() {
        let kinds: Vec<String> = receipt
            .unknown_kinds
            .iter()
            .map(|count| format!("`{}` ×{}", count.kind, count.events))
            .collect();
        out.push_str(&format!(
            "- {} event kind(s) this binary does not know — interpreted partially: {}\n",
            mark(false),
            kinds.join(", ")
        ));
    }
    match &receipt.baseline {
        Some(b) => out.push_str(&format!(
            "- {} {} regression(s) vs baseline across {} comparison(s) (suite `{}`, hash `{}`)\n",
            mark(b.regressions == 0),
            b.regressions,
            b.compared,
            b.suite,
            b.hash.as_str().get(..12).unwrap_or_default()
        )),
        None => out.push_str("- baseline: not used by this workflow\n"),
    }
    out.push_str(&format!(
        "- {} scope: {} file(s) touched, {} violation(s)\n",
        mark(receipt.scope.violations.is_empty()),
        receipt.scope.files_touched,
        receipt.scope.violations.len()
    ));
    let groups = fan_out_groups(&receipt.runners);
    if groups.is_empty() {
        out.push_str(&format!(
            "- {} runner(s) used, no fan-out review\n",
            receipt.runners.len()
        ));
    } else {
        for (base, members) in &groups {
            let adapters: Vec<&str> = members.iter().map(|m| m.adapter.as_str()).collect();
            out.push_str(&format!(
                "- ✓ Reviewed by {} independent runner(s) via `{base}` ({})\n",
                members.len(),
                adapters.join(", ")
            ));
        }
    }
    out.push_str(&format!(
        "- cost: {} tokens ({} in / {} out){} · {} reroute(s)\n",
        receipt.cost.tokens.total(),
        receipt.cost.tokens.input,
        receipt.cost.tokens.output,
        match receipt.cost.cptv {
            Some(cptv) => format!(" · CPTV: {cptv:.1} tokens/task"),
            None => String::new(),
        },
        receipt.cost.reroutes,
    ));
    match &receipt.event_chain {
        EventChainStatus::Intact { events } => out.push_str(&format!(
            "- ✓ event chain: {events} event(s), hash-linked, replayable\n"
        )),
        EventChainStatus::Broken { seq, detail } => {
            out.push_str(&format!("- ✗ event chain BROKEN at seq {seq}: {detail}\n"))
        }
    }

    if !receipt.criteria.entries.is_empty() {
        out.push_str("\n## Criteria\n\n");
        for entry in &receipt.criteria.entries {
            out.push_str(&format!(
                "- {} `{}` — `{}` (exit {})\n",
                mark(entry.exit_code == 0),
                entry.task_id,
                entry.cmd,
                entry.exit_code
            ));
        }
    }
    if !receipt.scope.violations.is_empty() {
        out.push_str("\n## Scope violations\n\n");
        for path in &receipt.scope.violations {
            out.push_str(&format!("- {}\n", path.display()));
        }
    }

    out
}

fn mark(ok: bool) -> &'static str {
    if ok {
        "✓"
    } else {
        "✗"
    }
}

/// JSON rendering — the machine-consumable format: same data as the
/// markdown, no separate derivation.
pub fn render_json(receipt: &Receipt) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(receipt)
}
