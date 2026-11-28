//! Verified Work Receipt: a PR-attachable
//! certificate derived **entirely** from the event log — never a summary
//! an agent wrote. "The receipt IS the evidence": every number here traces
//! back to a specific event kind, the same discipline [`crate::stats`]
//! and [`crate::RunState::replay`] hold to. No new bookkeeping — this
//! module reads the ledgers the log folds into, once, and walks no event
//! of its own: a second reading of the events would be a second answer.
//! Drawing it for a person is for whoever shows it; this hands over the
//! data.

use std::collections::{BTreeMap, HashMap};

use yunta_core::diagnostic::{ArtifactFailure, DiagnosticCode, DocumentKind};
use yunta_core::events::{
    CriterionType, EngineFact, Failure, NodeOutcome, NodeState, RunMetrics, StoredEvent,
    TerminalState,
};
use yunta_core::{CheckBuiltin, Manifest, NodeId, NodeKind, RunId, Seq};

use crate::RunState;

use yunta_core::receipt::{
    BaselineSummary, CostSummary, CriteriaSummary, CriterionEntry, DiagnosticCount,
    EventChainStatus, FailedNode, FindingsSummary, OpenedPullRequest, Receipt, RunnerUsage,
    ScopeSummary,
};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ReceiptError {
    /// A receipt certifies *closed* work — a run still
    /// `running`/`waiting`/`paused` has no
    /// `run_finished` metrics (CPTV, final token total) to report.
    ///
    /// The sentence names the state the run is in and stops there:
    /// which command shows a reader where that run stands is the
    /// caller's own vocabulary, not this crate's.
    #[error(
        "run `{0}` hasn't reached a terminal state yet: a receipt is only generated \
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
fn diagnostic_counts(state: &RunState) -> Vec<DiagnosticCount> {
    let mut counts: HashMap<(Option<DocumentKind>, DiagnosticCode), usize> = HashMap::new();
    let failures = state
        .nodes
        .values()
        .flat_map(|record| record.failures.iter());
    for failure in failures {
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

/// The receipt of a closed run, purely from `manifest` + `events` (+ the
/// chain status the caller already computed via `Storage::verify_chain`,
/// since that alone needs IO). Pure otherwise — same log, same receipt,
/// always.
pub fn closed(
    run_id: &RunId,
    manifest: &Manifest,
    events: &[StoredEvent],
    event_chain: EventChainStatus,
) -> Result<Receipt, ReceiptError> {
    closed_as_kept(run_id, manifest, &RunState::replay(events), event_chain)
}

/// The receipt of a closed run as [`closed`] derives it, from `state`, the state its events
/// fold into, as a reader that keeps the fold holds it: nothing is replayed. What a run's
/// screen shows of a run it follows.
pub fn closed_as_kept(
    run_id: &RunId,
    manifest: &Manifest,
    state: &RunState,
    event_chain: EventChainStatus,
) -> Result<Receipt, ReceiptError> {
    let Some(finished) = state.run.finished() else {
        return Err(ReceiptError::NotFinished(run_id.clone()));
    };
    Ok(receipt(
        run_id,
        manifest,
        state,
        event_chain,
        Some(finished.terminal),
        finished.metrics.clone(),
    ))
}

/// The receipt of a run still open, as its log stands: what it has held
/// its work to and found so far, with the cost a close now would record.
/// What a pull request a run opens as its last step carries.
pub fn so_far(
    run_id: &RunId,
    manifest: &Manifest,
    events: &[StoredEvent],
    event_chain: EventChainStatus,
) -> Receipt {
    let state = RunState::replay(events);
    let metrics = state.metrics();
    receipt(run_id, manifest, &state, event_chain, None, metrics)
}

/// The receipt of `state`'s run: every answer comes from its ledgers.
fn receipt(
    run_id: &RunId,
    manifest: &Manifest,
    state: &RunState,
    event_chain: EventChainStatus,
    terminal_state: Option<TerminalState>,
    metrics: RunMetrics,
) -> Receipt {
    let criteria = criteria_summary(state);
    let baseline = baseline_summary(manifest, state);
    let scope = scope_summary(state);
    let runners = runner_usage(state);
    let unknown_kinds = state.unknown_kinds();
    Receipt {
        schema_version: Receipt::SCHEMA_VERSION,
        run_id: run_id.clone(),
        workflow: manifest.workflow.name.clone(),
        mode: state.run.mode().clone(),
        terminal_state,
        criteria,
        baseline,
        scope,
        changed: state.current_change().cloned(),
        runners,
        cost: CostSummary {
            tokens: metrics.tokens,
            cptv: metrics.cptv,
            reroutes: state.nodes.reroutes(),
        },
        event_chain,
        unknown_kinds,
        diagnostics: diagnostic_counts(state),
        failed: failed_nodes(manifest, state),
        blocking_findings: state.findings_that_block().len(),
        findings: findings_summary(state),
        pull_request: pull_request(state),
    }
}

/// The findings that stand, as the findings' ledger keeps them, and how many a node
/// answered: what a receipt counts, and what a gate that ships the work says above its
/// card before the run closes.
pub fn findings_summary(state: &RunState) -> FindingsSummary {
    let standing = state.findings.standing().findings;
    FindingsSummary {
        posted: standing.len(),
        answered: standing
            .iter()
            .filter(|finding| !finding.answers.is_empty())
            .count(),
    }
}

/// The pull request a node of the run opened, as the node closed on it.
fn pull_request(state: &RunState) -> Option<OpenedPullRequest> {
    state.nodes.values().find_map(|record| match &record.state {
        Some(NodeState::Finished {
            outcome: NodeOutcome::Fact(EngineFact::PullRequestOpened { number, url }),
            ..
        }) => Some(OpenedPullRequest {
            number: *number,
            url: url.clone(),
        }),
        _ => None,
    })
}

/// Each node that stands failed, in the order the workflow declares it.
fn failed_nodes(manifest: &Manifest, state: &RunState) -> Vec<FailedNode> {
    manifest
        .workflow
        .iter_nodes()
        .filter_map(|node| match state.nodes.state(&node.id) {
            Some(NodeState::Failed { failure, .. }) => Some(FailedNode {
                node: node.id.clone(),
                failure: failure.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// Each task's check after its work, as the tasks' ledger keeps it: the
/// latest, so a retried task's earlier, superseded attempts don't get
/// counted twice. The log keeps every attempt, but the receipt certifies
/// only the final verdict, and a gate that ships the work says it above its
/// card before the run closes.
pub fn criteria_summary(state: &RunState) -> CriteriaSummary {
    let checked: Vec<_> = state
        .tasks
        .iter()
        .filter_map(|(task, record)| Some((task, record, record.checked.as_ref()?)))
        .collect();
    let mut summary = CriteriaSummary {
        total: 0,
        green: 0,
        tasks: checked.len(),
        proving: 0,
        proved: 0,
        red_before: 0,
        guards: 0,
        guards_green: 0,
        entries: Vec::new(),
    };
    for (task, record, after) in checked {
        for result in &after.results {
            let green = result.exit_code == 0;
            summary.total += 1;
            summary.green += usize::from(green);
            if result.r#type == Some(CriterionType::Guard) {
                summary.guards += 1;
                summary.guards_green += usize::from(green);
            } else {
                summary.proving += 1;
                summary.proved += usize::from(green);
                summary.red_before += usize::from(red_before(record, &result.cmd));
            }
            summary.entries.push(CriterionEntry {
                task_id: task.to_string(),
                cmd: result.cmd.clone(),
                exit_code: result.exit_code,
            });
        }
    }
    summary
        .entries
        .sort_by(|a, b| (&a.task_id, &a.cmd).cmp(&(&b.task_id, &b.cmd)));
    summary
}

/// Whether the task's check before its work found `cmd` red, as a criterion the work has
/// to make pass must be.
fn red_before(record: &yunta_core::events::TaskRecord, cmd: &str) -> bool {
    record.checked_before.as_ref().is_some_and(|before| {
        before
            .results
            .iter()
            .any(|result| result.cmd == cmd && result.exit_code != 0)
    })
}

fn baseline_summary(manifest: &Manifest, state: &RunState) -> Option<BaselineSummary> {
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

/// Every path the run's scope checks found changed, and those it may not
/// change, each once: the union the node ledger keeps over every check.
fn scope_summary(state: &RunState) -> ScopeSummary {
    let union = state.nodes.scope_union();
    let granted: Vec<_> = state
        .tasks
        .iter()
        .flat_map(|(task, _)| state.grants.person_granted_to_task(task).iter().cloned())
        .collect();
    // A glob the log holds was checked when it was granted; one that no longer compiles
    // grants nothing here.
    let granted = yunta_core::scope_globset(&granted).ok();
    ScopeSummary {
        files_touched: union.diff.len(),
        violations: union.violations.iter().cloned().collect(),
        granted: union
            .diff
            .iter()
            .filter(|path| granted.as_ref().is_some_and(|set| set.is_match(path)))
            .count(),
    }
}

/// Which runner ran each node, in the order the nodes first resolved
/// one.
///
/// A node resolves its runner once per session it opens — again after a
/// reroute, again on a retry — and the receipt's line counts
/// runners, not resolutions, so each node appears once. Fan-out siblings
/// carry distinct ids (`<base>@<runner>`), so they are not collapsed by
/// this. A runner that names no model ran on the one its CLI chose, which
/// the node's first session reported.
fn runner_usage(state: &RunState) -> Vec<RunnerUsage> {
    // Keyed by the seq of each node's first resolution, so the map reads
    // in the order the nodes first resolved one.
    let by_resolution: BTreeMap<Seq, RunnerUsage> = state
        .nodes
        .iter()
        .filter_map(|(node_id, record)| {
            let (seq, resolved) = record.first_runner.as_ref()?;
            let usage = RunnerUsage {
                node_id: node_id.clone(),
                runner: resolved.runner.clone(),
                adapter: resolved.chosen.adapter.clone(),
                model: resolved
                    .chosen
                    .model
                    .clone()
                    .or_else(|| record.models.first().cloned()),
            };
            Some((*seq, usage))
        })
        .collect();
    by_resolution.into_values().collect()
}
