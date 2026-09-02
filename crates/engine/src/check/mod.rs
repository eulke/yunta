//! `yunta check`.
//!
//! [`check`] validates one workflow file against the merged config and
//! **never reads other files**: node-id uniqueness (every `parallel`
//! child included), `depends_on` references and acyclicity (never
//! relaxed by `on_failure.goto`, a separate edge set), goto/gate/mode
//! reference integrity, runner resolution, scope disjointness for
//! `parallel` and DAG fan-out, permission ceilings over
//! literal commands, input specs and references,
//! `yunta_schema`, and workflow-node and fan-out
//! declaration rules. [`check_workflow_refs`] is the deliberate
//! exception that does read files: the composition reference graph
//! (`use:` resolves, acyclic, within `limits.max_workflow_depth`)
//! against the repo's `.yunta/workflows/` catalog — a separate entry
//! point so `check`'s no-IO property stays intact, called alongside it
//! by the CLI.
//!
//! Capability-aware checks (agent existence, required
//! capabilities) wait for the `Adapter` trait to exist — there is
//! nothing to probe yet.
//!
//! The rules live in families — one submodule per subject; this file owns
//! the two entries that run them all and the shared re-exports each family
//! reads through `use super::*`.

mod declarations;
mod error;
mod gates;
mod graph;
mod inputs;
mod modes;
mod packs;
mod refs;
mod runners;
mod scopes;

pub use error::{CheckError, CheckWarning, SchemaRangeError};
pub use refs::check_workflow_refs;

// One home for what every family reads: the workspace types, the two
// cross-crate helpers, and each family's own rule functions, so a family
// file's `use super::*` sees them all and the two entries below call any
// rule unqualified.
pub(crate) use crate::ledger::globs_might_overlap;
pub(crate) use crate::template::template_variables;
pub(crate) use declarations::*;
pub(crate) use gates::*;
pub(crate) use graph::*;
pub(crate) use inputs::*;
pub(crate) use modes::*;
pub(crate) use packs::*;
pub(crate) use runners::*;
pub(crate) use scopes::*;
pub(crate) use std::collections::{HashMap, HashSet};
pub(crate) use yunta_core::{
    ConfigLayer, InputSpec, ModeName, Node, NodeId, NodeKind, RunnerName, Workflow,
};

/// The pseudo-node a finding about `node_defaults:` is attributed to.
pub(crate) static NODE_DEFAULTS: NodeId = NodeId::from_static("node_defaults");

/// The pseudo-node a finding about the config's `defaults:` is attributed to.
pub(crate) static DEFAULTS: NodeId = NodeId::from_static("defaults");

/// Validates a workflow against the full rule set. Every applicable rule is
/// checked and every violation reported — not just the first one (same
/// spirit as the ledger: whoever writes this by hand corrects
/// once, not once per `yunta check` run).
pub fn check(workflow: &Workflow, config: &ConfigLayer) -> Vec<CheckError> {
    // `context: [{ artifact }]` creates an implicit `depends_on` edge
    // — expanded here, on this function's own clone, so cycle detection
    // below sees exactly the graph a real run would build (`build_manifest`
    // expands the same way), never a narrower one that misses a cycle
    // formed only through context references.
    let mut workflow = workflow.clone();
    let mut errors = Vec::new();
    // Fan-out declarations validate on the *original* shape (the
    // rules are about the declaration itself), then the graph expands so
    // every later rule sees what will actually run.
    check_runner_fanout(&workflow, &mut errors);
    // Workflow-node rules also validate the original shape
    // (`runners:` on one is refused before expansion would multiply it).
    check_workflow_nodes(&workflow.nodes, None, &mut errors);
    // Mount declarations too — expansion below turns each mount
    // into an ordinary `depends_on` edge, so cycle detection sees them.
    check_mounts(&workflow, &mut errors);
    crate::manifest::expand_runner_fanout(&mut workflow);
    crate::manifest::expand_implicit_dependencies(&mut workflow);
    let workflow = &workflow;

    // Global, not per-group: replay derives node state from one flat
    // NodeId -> NodeState map, so a `parallel` child's id colliding
    // with anything else — a sibling, a top-level node, another group's
    // child — would corrupt derivation, not just read oddly.
    let mut known_ids: HashSet<NodeId> = HashSet::new();
    for node in workflow.iter_nodes() {
        if !known_ids.insert(node.id.clone()) {
            errors.push(CheckError::DuplicateNodeId {
                id: node.id.clone(),
            });
        }
    }

    check_parallel_scopes(&workflow.nodes, &mut errors);
    check_fanout_scopes(workflow, config, &mut errors);
    check_resume_session(workflow, &mut errors);
    check_yunta_schema(workflow, &mut errors);
    check_config_defaults(config, &mut errors);
    check_distill_paths(workflow, &mut errors);
    check_artifact_names(workflow, &mut errors);

    if let Some(permissions) = &config.permissions {
        check_commands(&workflow.nodes, permissions, &mut errors);
        if let Some(default_hooks) = workflow
            .node_defaults
            .as_ref()
            .and_then(|defaults| defaults.hooks.as_ref())
        {
            // node_defaults hooks run on every node that declares none of
            // its own — their commands are as real as any node's.
            for step in default_hooks.before.iter().chain(&default_hooks.after) {
                if let Some(rule) =
                    crate::permissions::command_violation(&step.run, Some(permissions))
                {
                    errors.push(CheckError::CommandDenied {
                        node: NODE_DEFAULTS.clone(),
                        rule,
                    });
                }
            }
        }
    }

    for node in &workflow.nodes {
        for dep in &node.depends_on {
            if !known_ids.contains(dep) {
                errors.push(CheckError::BrokenReference {
                    node: node.id.clone(),
                    field: "depends_on".to_string(),
                    target: dep.clone(),
                });
            }
        }

        if let Some(on_failure) = &node.on_failure {
            if !known_ids.contains(&on_failure.goto) {
                errors.push(CheckError::BrokenReference {
                    node: node.id.clone(),
                    field: "on_failure.goto".to_string(),
                    target: on_failure.goto.clone(),
                });
            }
        }

        if let Some(runner) = &node.runner {
            match config.runners.as_ref().and_then(|r| r.get(runner)) {
                None => errors.push(CheckError::UnknownRunner {
                    node: node.id.clone(),
                    runner: runner.clone(),
                }),
                Some(candidates) if candidates.is_empty() => {
                    errors.push(CheckError::RunnerHasNoCandidates {
                        node: node.id.clone(),
                        runner: runner.clone(),
                    })
                }
                Some(_) => {}
            }
        }

        if !node.context.is_empty()
            && !matches!(node.kind, NodeKind::Prompt { .. } | NodeKind::Loop { .. })
        {
            errors.push(CheckError::ContextOnUnsupportedNode {
                node: node.id.clone(),
            });
        }

        check_gate(node, &known_ids, config, &mut errors);

        // The loop's declared expansion mode against the
        // merged ceiling. An absent block is `deny` — the strictest —
        // so only an explicit, too-permissive declaration can trip.
        if let NodeKind::Loop {
            scope_expansion: Some(se),
            ..
        } = &node.kind
        {
            if let Some(ceiling) = config
                .permissions
                .as_ref()
                .and_then(|permissions| permissions.scope_expansion)
            {
                if se.mode.strictness() < ceiling.max_mode.strictness() {
                    errors.push(CheckError::ScopeExpansionModeOverCeiling {
                        node: node.id.clone(),
                        mode: se.mode.as_str(),
                        ceiling: ceiling.max_mode.as_str(),
                    });
                }
            }
        }
    }

    check_no_gate_in_parallel(&workflow.nodes, None, &mut errors);

    if let Some(cycle) = find_depends_on_cycle(&workflow.nodes) {
        let path = cycle
            .iter()
            .map(NodeId::as_str)
            .collect::<Vec<_>>()
            .join(" -> ");
        errors.push(CheckError::DependsOnCycle { path });
    }

    check_input_specs(&workflow.inputs, &mut errors);
    check_input_references(workflow, &mut errors);
    check_modes(workflow, &mut errors);

    errors
}

/// Non-blocking findings — the "can't verify, so warn" case.
/// Separate entry point from [`check`] rather than a severity field on
/// `CheckError`, so nothing that already treats `check()`'s output as
/// "must be empty to proceed" has to learn to filter by severity.
///
/// The real condition is "two or more children **with write
/// permissions**" — nodes declare `permissions:
/// read-only|edit|full`, so a child declaring `read-only` is out of the
/// collision count by declaration. A child without the field stays
/// implicitly write-capable (the engine's default profile is `edit`).
pub fn check_warnings(workflow: &Workflow, config: &ConfigLayer) -> Vec<CheckWarning> {
    let mut warnings = Vec::new();
    collect_parallel_warnings(&workflow.nodes, &mut warnings);
    collect_fanout_warnings(workflow, config, &mut warnings);
    collect_push_to_base_warnings(workflow, config, &mut warnings);
    warnings
}
