//! Verified Work Receipt: a PR-attachable
//! certificate derived **entirely** from the event log — never a summary
//! an agent wrote. "El recibo ES la evidencia": every number here traces
//! back to a specific event kind, the same discipline [`crate::stats`]
//! and [`crate::progress`] already hold to. No new bookkeeping — this
//! module only reads what the engine already recorded. Drawing it for a
//! person is for whoever shows it; the engine hands over the data.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use yunta_core::diagnostic::{ArtifactFailure, DiagnosticCode, DocumentKind};
use yunta_core::events::{EventPayload, Failure, Phase, StoredEvent};
use yunta_core::{CheckBuiltin, Manifest, NodeId, NodeKind, RunId};

use crate::replay::unknown_kind_counts;
use crate::replay::{derive, NodeState};
use yunta_core::events::{NodeEvent, RunEvent};

pub use yunta_core::receipt::{
    BaselineSummary, CostSummary, CriteriaSummary, CriterionEntry, DiagnosticCount,
    EventChainStatus, Receipt, RunnerUsage, ScopeSummary,
};

/// The receipt as a program reads it: the same data a person's document
/// is drawn from, no separate derivation.
pub fn render_json(receipt: &Receipt) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(receipt)
}

#[derive(Debug, thiserror::Error)]
pub enum ReceiptError {
    /// A receipt certifies *closed* work — a run still
    /// `running`/`waiting`/`paused` has no
    /// `run_finished` metrics (CPTV, final token total) to report yet.
    ///
    /// The sentence names the state the run is in and stops there:
    /// which command shows a reader where that run stands is the
    /// caller's own vocabulary, not the engine's.
    #[error(
        "run `{0}` hasn't reached a terminal state yet — a receipt is only generated \
         once a run finishes"
    )]
    NotFinished(RunId),
}

/// What one artifact failure adds to the count: the `(document kind,
/// code)` of each problem it carries.
///
/// A problem with the artifact itself is counted under its own code and
/// no kind: `artifact-missing` is the same fact whatever the file was
/// going to contain, `artifact-undelivered` the same whatever the node
/// was going to hand over, and `artifact-unheld` the same whatever run
/// was asked. A document whose content failed is not one problem but
/// every problem it has, each under the kind it was read against.
fn counted(failure: &ArtifactFailure) -> Vec<(Option<DocumentKind>, DiagnosticCode)> {
    match failure {
        // Exhaustive rather than keyed off `report()`, so a fifth way an
        // artifact can fail reaches this decision as a compile error
        // instead of falling into whichever arm happens to fit.
        ArtifactFailure::File { .. }
        | ArtifactFailure::Undelivered { .. }
        | ArtifactFailure::Unheld { .. } => failure
            .code()
            .map(|code| (None, code))
            .into_iter()
            .collect(),
        ArtifactFailure::Content(report) => report
            .diagnostics
            .iter()
            .map(|diagnostic| (Some(report.document.kind), diagnostic.code()))
            .collect(),
    }
}

/// Every problem the log recorded, counted by `(document kind, code)`,
/// most frequent first and ties broken by name so the same log always
/// renders the same receipt.
fn diagnostic_counts(events: &[StoredEvent]) -> Vec<DiagnosticCount> {
    let mut counts: HashMap<(Option<DocumentKind>, DiagnosticCode), usize> = HashMap::new();
    let failed = events.iter().filter_map(|event| match event.payload() {
        Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(&p.failure),
        _ => None,
    });
    for failure in failed {
        // Only a failure about documents has documents to count. A
        // sentence and a dead session name no artifact, and counting
        // them as zero of something would be a different claim from
        // having nothing to say.
        let Failure::Artifacts { artifacts } = failure else {
            continue;
        };
        for entry in artifacts.iter().flat_map(counted) {
            *counts.entry(entry).or_default() += 1;
        }
    }
    let mut counts: Vec<DiagnosticCount> = counts
        .into_iter()
        .map(|((kind, code), occurrences)| DiagnosticCount {
            kind,
            code,
            occurrences,
        })
        .collect();
    counts.sort_by(|a, b| {
        b.occurrences
            .cmp(&a.occurrences)
            .then_with(|| a.code.as_str().cmp(b.code.as_str()))
            .then_with(|| {
                a.kind
                    .map(DocumentKind::label)
                    .cmp(&b.kind.map(DocumentKind::label))
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
        Some(EventPayload::Run(RunEvent::Finished(p))) => {
            Some((p.terminal_state, p.metrics.clone()))
        }
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
        .filter(|e| {
            matches!(
                e.payload(),
                Some(EventPayload::Node(NodeEvent::Rerouted(_)))
            )
        })
        .count();

    Ok(Receipt {
        schema_version: Receipt::SCHEMA_VERSION,
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
    })
}

/// The latest post-check `criteria_checked` per task — a retried task's
/// earlier, superseded attempts don't get counted twice: the log keeps
/// every attempt, but the receipt certifies only the final verdict.
fn criteria_summary(events: &[StoredEvent]) -> CriteriaSummary {
    let mut latest_post: HashMap<String, &yunta_core::events::CriteriaCheckedPayload> =
        HashMap::new();
    for event in events {
        if let Some(EventPayload::Node(NodeEvent::CriteriaChecked(p))) = event.payload() {
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
    // A run holds a measurement whenever its lineage declared a suite,
    // so what decides whether the run *looked* is the workflow: with no
    // `baseline_compare` in it there is no comparison to report, and a
    // "0 regressions" line would be about nothing.
    let compares: Vec<&NodeId> = manifest
        .workflow
        .iter_nodes()
        .filter(|node| matches!(&node.kind, NodeKind::Check(CheckBuiltin::BaselineCompare)))
        .map(|node| &node.id)
        .collect();
    if compares.is_empty() {
        return None;
    }
    // Read from the run's own fold and each node's derived state, never
    // from a node's outcome text.
    let state = derive(events);
    let captured = state.run.baseline()?;
    let mut compared = 0usize;
    let mut regressions = 0usize;
    for node in compares {
        match state.nodes.state(node) {
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
        origin: captured.origin.clone(),
        red: (!captured.passed()).then_some(captured.results.exit_code),
    })
}

fn scope_summary(events: &[StoredEvent]) -> ScopeSummary {
    let mut files: BTreeSet<PathBuf> = BTreeSet::new();
    let mut violations: BTreeSet<PathBuf> = BTreeSet::new();
    for event in events {
        if let Some(EventPayload::Node(NodeEvent::ScopeChecked(p))) = event.payload() {
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
/// A node resolves its runner once per session it opens — again after a
/// re-route, again on a retry — and the receipt's line counts
/// runners, not resolutions, so each node appears once. Fan-out siblings
/// carry distinct ids (`<base>@<runner>`), so they are not collapsed by
/// this.
fn runner_usage(events: &[StoredEvent]) -> Vec<RunnerUsage> {
    let mut seen: BTreeSet<NodeId> = BTreeSet::new();
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::Node(NodeEvent::RunnerResolved(p))) => {
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
