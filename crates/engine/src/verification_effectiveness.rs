//! Verification-performance findings (§8.7, D93, T7.10) — the same
//! judgment the engine applies to a task's own work ("si algo no puede
//! fallar, no está probando nada") applied to the workflow's own
//! ceremony. Pure: takes a workflow and every past run's raw log,
//! returns findings — no I/O, no mutation, nothing that ever touches a
//! workflow file (§8.7's own third guard: "sugiere, jamás actúa").
//!
//! **Core metric is the pre-check red rate, not the total failure
//! rate** — confusing the two would suggest deleting exactly the
//! criteria doing their job best (§8.7's own text). A criterion that's
//! red before the work and green after, every time, is working exactly
//! as intended and never appears here.
//!
//! **Evidence threshold is per criterion/re-route/gate, never per
//! workflow**: [`MIN_SAMPLES`] gates each signal independently, so a
//! criterion just added to a workflow with 50 historical runs starts
//! with zero samples of its own, not the workflow's history.
//!
//! **Mode-scoped signals (DI-06, unblocked by T9.1)**: a declared mode
//! no run ever chose (with the same evidence floor), and the structural
//! guard §8.7 demands — an `invariant: true` node is never the subject
//! of a remove-shaped finding (its never-fired re-route or
//! always-approved gate is the node doing its job, excluded by
//! construction).

use std::collections::HashMap;

use yunta_core::events::{Event, EventPayload, Phase};
use yunta_core::{NodeId, Workflow};

/// Below this many independent samples, a signal says nothing — a
/// number without a distribution behind it is a guess wearing a data
/// costume (same floor, and the same reasoning, as §8.6's own prior
/// estimation, T7.5).
pub const MIN_SAMPLES: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct NeverRedCriterion {
    pub cmd: String,
    pub sample_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NeverTriggeredReroute {
    pub node: NodeId,
    pub goto: NodeId,
    pub sample_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlwaysApprovedGate {
    pub node: NodeId,
    pub sample_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlwaysFirstTryTasks {
    pub sample_count: usize,
}

/// A declared mode no historical run ever chose (§8.7 "modo sin uso") —
/// evidence is "enough runs happened and none picked it", never "it has
/// existed a long time".
#[derive(Debug, Clone, PartialEq)]
pub struct UnusedMode {
    pub name: String,
    pub runs_observed: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VerificationFindings {
    pub never_red_criteria: Vec<NeverRedCriterion>,
    pub never_triggered_reroutes: Vec<NeverTriggeredReroute>,
    pub always_approved_gates: Vec<AlwaysApprovedGate>,
    pub always_first_try_tasks: Option<AlwaysFirstTryTasks>,
    pub unused_modes: Vec<UnusedMode>,
}

impl VerificationFindings {
    pub fn is_empty(&self) -> bool {
        self.never_red_criteria.is_empty()
            && self.never_triggered_reroutes.is_empty()
            && self.always_approved_gates.is_empty()
            && self.always_first_try_tasks.is_none()
            && self.unused_modes.is_empty()
    }
}

/// `history` is every past run's own full event log — the caller's job
/// (imperative shell) is gathering those; this function only reads them.
pub fn analyze(workflow: &Workflow, history: &[Vec<Event>]) -> VerificationFindings {
    VerificationFindings {
        never_red_criteria: never_red_criteria(history),
        never_triggered_reroutes: never_triggered_reroutes(workflow, history),
        always_approved_gates: always_approved_gates(workflow, history),
        always_first_try_tasks: always_first_try_tasks(history),
        unused_modes: unused_modes(workflow, history),
    }
}

/// §8.7 "modo sin uso" (DI-06): every declared mode against the mode
/// each historical run actually recorded in its own `run_created`.
fn unused_modes(workflow: &Workflow, history: &[Vec<Event>]) -> Vec<UnusedMode> {
    let Some(modes) = &workflow.modes else {
        return Vec::new();
    };
    if history.len() < MIN_SAMPLES {
        return Vec::new();
    }
    let used: std::collections::HashSet<&str> = history
        .iter()
        .filter_map(|events| {
            events.iter().find_map(|e| match &e.payload {
                EventPayload::RunCreated(p) => Some(p.mode.as_str()),
                _ => None,
            })
        })
        .collect();
    modes
        .keys()
        .filter(|name| !used.contains(name.as_str()))
        .map(|name| UnusedMode {
            name: name.clone(),
            runs_observed: history.len(),
        })
        .collect()
}

/// Every `criteria_checked` result, of any phase — pre-check red rate is
/// what §8.7 names as the metric, but a criterion that's *only* ever
/// checked post (never pre, e.g. a node with no explicit pre-check
/// declared) still deserves the same "never red" reading if that's
/// literally true of every observation this codebase has of it. Reused
/// (memoized, T5.9) results count too: a reused verdict is still a real
/// exit code for that command, not a different answer standing in for
/// one.
fn never_red_criteria(history: &[Vec<Event>]) -> Vec<NeverRedCriterion> {
    let mut samples: HashMap<String, (usize, usize)> = HashMap::new(); // cmd -> (total, red)
    for events in history {
        for event in events {
            let EventPayload::CriteriaChecked(p) = &event.payload else {
                continue;
            };
            if p.phase != Phase::Pre {
                continue;
            }
            for result in &p.results {
                let entry = samples.entry(result.cmd.clone()).or_default();
                entry.0 += 1;
                if result.exit_code != 0 {
                    entry.1 += 1;
                }
            }
        }
    }
    let mut findings: Vec<NeverRedCriterion> = samples
        .into_iter()
        .filter(|(_, (total, red))| *total >= MIN_SAMPLES && *red == 0)
        .map(|(cmd, (total, _))| NeverRedCriterion {
            cmd,
            sample_count: total,
        })
        .collect();
    findings.sort_by(|a, b| a.cmd.cmp(&b.cmd));
    findings
}

/// One sample per historical run where the node actually ran (reached
/// *any* terminal state, finished or failed) — not per run where it
/// failed: a node that finished clean every single time is exactly
/// "the prior flow is more reliable than expected" (§8.7's own
/// reading), the strongest possible version of "this re-route never
/// fired". A run where the node never even executed (blocked behind
/// something else entirely) says nothing about its re-route either
/// way, so it isn't counted as a sample.
fn never_triggered_reroutes(
    workflow: &Workflow,
    history: &[Vec<Event>],
) -> Vec<NeverTriggeredReroute> {
    let mut findings = Vec::new();
    for node in flatten(&workflow.nodes) {
        // §8.7's own structural guard (T7.10 ✓): an invariant node that
        // never fails is verification doing its job — never a removal
        // candidate, so never a finding.
        if node.invariant {
            continue;
        }
        let Some(on_failure) = &node.on_failure else {
            continue;
        };
        let mut eligible = 0usize;
        let mut triggered = 0usize;
        for events in history {
            let ran = events.iter().any(|e| {
                matches!(
                    e.payload,
                    EventPayload::NodeFinished(_) | EventPayload::NodeFailed(_)
                ) && e.node_id.as_ref() == Some(&node.id)
            });
            if !ran {
                continue;
            }
            eligible += 1;
            let rerouted = events.iter().any(|e| {
                matches!(&e.payload, EventPayload::NodeRerouted(p) if p.to_node == on_failure.goto)
                    && e.node_id.as_ref() == Some(&node.id)
            });
            if rerouted {
                triggered += 1;
            }
        }
        if eligible >= MIN_SAMPLES && triggered == 0 {
            findings.push(NeverTriggeredReroute {
                node: node.id.clone(),
                goto: on_failure.goto.clone(),
                sample_count: eligible,
            });
        }
    }
    findings
}

/// A gate "needed adjustment" when its resolution reroutes (T7.2's
/// internal `retry`, or T7.7's external changes-requested — both land
/// as `gate_resolved.chosen_option: Some("retry")` or a `node_failed`
/// right after it) rather than letting the node finish clean. One
/// sample per `gate_resolved` this node ever emitted, across history.
fn always_approved_gates(workflow: &Workflow, history: &[Vec<Event>]) -> Vec<AlwaysApprovedGate> {
    let gate_node_ids: Vec<NodeId> = flatten(&workflow.nodes)
        .into_iter()
        .filter(|n| matches!(n.kind, yunta_core::NodeKind::Gate { .. }) && !n.invariant)
        .map(|n| n.id.clone())
        .collect();
    // Also every node with `on_failure` — T7.2's internal gate escalates
    // *that* node once its re-routes are exhausted, so it's a gate too,
    // by the same §5.3 object, even without `kind: gate`.
    let internal_gate_ids: Vec<NodeId> = flatten(&workflow.nodes)
        .into_iter()
        .filter(|n| n.on_failure.is_some() && !n.invariant && !gate_node_ids.contains(&n.id))
        .map(|n| n.id.clone())
        .collect();

    let mut findings = Vec::new();
    for node_id in gate_node_ids.into_iter().chain(internal_gate_ids) {
        let mut total = 0usize;
        let mut needed_adjustment = 0usize;
        for events in history {
            for event in events {
                let EventPayload::GateResolved(p) = &event.payload else {
                    continue;
                };
                if event.node_id.as_ref() != Some(&node_id) {
                    continue;
                }
                total += 1;
                // T7.7's own mapping: `approved_sha` is only ever set on
                // a clean approval — an internal "retry"/"abort", or an
                // external changes-requested/closed, all leave it
                // `None`, and all of them are an adjustment.
                if p.approved_sha.is_none() {
                    needed_adjustment += 1;
                }
            }
        }
        if total >= MIN_SAMPLES && needed_adjustment == 0 {
            findings.push(AlwaysApprovedGate {
                node: node_id,
                sample_count: total,
            });
        }
    }
    findings
}

/// Workflow-level, not per-task: task identity isn't durable across
/// separately-planned runs (a fresh ledger can renumber `T001`), so
/// "this exact task always passes first try" isn't a claim the log can
/// support across runs — whether *any* task anywhere needed a retry is.
/// One sample per task instance across every historical run's ledger.
fn always_first_try_tasks(history: &[Vec<Event>]) -> Option<AlwaysFirstTryTasks> {
    let mut total = 0usize;
    let mut needed_retry = 0usize;
    for events in history {
        let mut post_checks: HashMap<yunta_core::TaskId, usize> = HashMap::new();
        for event in events {
            let EventPayload::CriteriaChecked(p) = &event.payload else {
                continue;
            };
            if p.phase == Phase::Post {
                *post_checks.entry(p.task_id.clone()).or_default() += 1;
            }
        }
        for attempts in post_checks.into_values() {
            total += 1;
            if attempts > 1 {
                needed_retry += 1;
            }
        }
    }
    if total >= MIN_SAMPLES && needed_retry == 0 {
        Some(AlwaysFirstTryTasks {
            sample_count: total,
        })
    } else {
        None
    }
}

/// Every node in declaration order, `parallel` children included — same
/// convention `progress.rs`/`stats.rs` each already duplicate locally
/// (neither module exports it; a node's tree position isn't part of
/// either's public surface).
fn flatten(nodes: &[yunta_core::Node]) -> Vec<&yunta_core::Node> {
    let mut flat = Vec::new();
    for node in nodes {
        flat.push(node);
        if let yunta_core::NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            flat.extend(flatten(children));
        }
    }
    flat
}
