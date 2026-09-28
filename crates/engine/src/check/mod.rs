//! `yunta check`.
//!
//! **What refuses and what warns.** A run freezes its workflow and its
//! config when it is created and starts from one commit, so whatever
//! those make certain to stop it is known before the first token — and
//! is an error: a key a node cannot run without, a read nothing in the
//! run can answer, a comparison with no suite to measure, a file nothing
//! before its reader can write, what a pack requires and the machine
//! lacks. A warning is for what the run may still get through — a risk,
//! a waste, a case the author may be right about, or one only the run
//! can settle — and it says what would make it certain. A rule that
//! cannot tell which it is looking at warns; one that can refuses.
//!
//! [`check`] validates one workflow file against the merged config and
//! **never reads other files**: node-id uniqueness (every `parallel`
//! child included), `depends_on` references and acyclicity (never
//! relaxed by `on_failure.goto`, a separate edge set), goto/gate/mode
//! reference integrity, runner resolution, scope disjointness for
//! `parallel` and DAG fan-out, permission ceilings over
//! literal commands, input specs and references,
//! `yunta_schema`, and workflow-node and fan-out
//! declaration rules. Two separate entry points are the deliberate
//! exceptions that do read files, so `check`'s no-IO property stays
//! intact and the CLI calls them alongside it: [`check_workflow_refs`],
//! the composition reference graph (`use:` resolves, acyclic, within
//! `limits.max_workflow_depth`) against the repo's `.yunta/workflows/`
//! catalog; and [`check_context_files`], whether the `files:` a node
//! reads are in the tree a run would start from.
//!
//! What a node declares of its adapter is checked against what this
//! binary builds: a declaration no candidate adapter can honor is
//! refused.
//!
//! The rules live in families — one submodule per subject; this file owns
//! the two entries that run them all and the shared re-exports each family
//! reads through `use super::*`.

mod capabilities;
mod context_files;
mod declarations;
mod error;
mod gates;
mod graph;
mod inputs;
mod packs;
mod programs;
mod refs;
mod runners;
mod scopes;
mod sources;
mod unset;
mod warning;

pub use context_files::{
    check_context_files, ContextFilesCheck, MissingContextFile, RunTreeOrigin,
};
pub use error::CheckError;
pub use programs::programs_named;
pub use refs::{check_workflow_refs, RefsCheck};
pub use sources::Unanswerable;
pub use warning::CheckWarning;

// One home for what every family reads: the workspace types, the
// shared helpers (the glob heuristic the tasks document's own scope rule uses,
// and the template scanner), and each family's own rule functions, so a
// family file's `use super::*` sees them all and the two entries below
// call any rule unqualified.
pub(crate) use declarations::*;
pub(crate) use gates::*;
pub(crate) use graph::*;
pub(crate) use inputs::*;
pub(crate) use packs::*;
pub(crate) use runners::*;
pub(crate) use scopes::*;
pub(crate) use sources::*;
pub(crate) use std::collections::{HashMap, HashSet};
pub(crate) use unset::*;
pub(crate) use yunta_core::template::template_variables;
pub(crate) use yunta_core::{
    might_overlap, ArtifactSpec, ConfigLayer, InputSpec, Node, NodeId, NodeKind, RunnerName,
    Workflow,
};

/// The pseudo-node a finding about `node_defaults:` is attributed to.
pub(crate) static NODE_DEFAULTS: NodeId = NodeId::from_static("node_defaults");

/// The pseudo-node a finding about the config's `defaults:` is attributed to.
pub(crate) static DEFAULTS: NodeId = NodeId::from_static("defaults");

/// Validates a workflow against the full rule set. Every applicable rule is
/// checked and every violation reported — not just the first one (same
/// spirit as the tasks document: whoever writes this by hand corrects
/// once, not once per `yunta check` run).
pub fn check(
    workflow: &Workflow,
    config: &ConfigLayer,
    declared: &dyn Fn(&yunta_core::AdapterId) -> Option<yunta_core::Capabilities>,
) -> Vec<CheckError> {
    check_mounted(workflow, config, declared, &[])
}

/// [`check`] for a run born holding what `mounts` carry in from the
/// workflow that composes it — what a composed workflow's reads may be
/// answered by, and a workflow a person starts never is.
pub fn check_mounted(
    workflow: &Workflow,
    config: &ConfigLayer,
    declared: &dyn Fn(&yunta_core::AdapterId) -> Option<yunta_core::Capabilities>,
    mounts: &[yunta_core::MountSpec],
) -> Vec<CheckError> {
    // `read` hands back the expanded graph, and this is asked of a
    // workflow that read — so the shape here is the one a run builds.
    // The clone is for the rules that are about the shape as *written*:
    // a fan-out declaration, a `kind: workflow` node's own, a mount's.
    let mut workflow = workflow.clone();
    let mut errors = Vec::new();
    check_runner_fanout(&workflow, &mut errors);
    check_workflow_nodes(&workflow.nodes, None, &mut errors);
    check_mounts(&workflow, &mut errors);
    yunta_core::workflow::read::expand_runner_fanout(&mut workflow);
    yunta_core::workflow::read::expand_implicit_dependencies(&mut workflow);
    let workflow = &workflow;
    // What a node asks of its adapter, checked against what this binary
    // built. `permissions:` and `agent:` have no fallback — refusing
    // here is the whole of that policy.
    capabilities::check_adapter_capabilities(workflow, config, declared, &mut errors);

    check_fanout_scopes(workflow, config, &mut errors);
    check_resume_session(workflow, &mut errors);
    check_yunta_schema(workflow, &mut errors);
    check_config_defaults(config, &mut errors);
    check_unset_keys(workflow, config, &mut errors);
    check_distill_paths(workflow, &mut errors);
    check_artifact_declarations(workflow, &mut errors);
    check_asking_nodes(workflow, &mut errors);
    check_input_documents(workflow, &mut errors);
    check_reserved_artifact_names(workflow, &mut errors);
    check_answer_sources(workflow, &mut errors);
    errors.extend(check_reads(workflow, &Birth::of(workflow, mounts), None).errors);
    check_named_artifact_sources(workflow, &mut errors);
    check_node_outputs(workflow, &mut errors);

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

        check_gate(node, config, &mut errors);

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
    check_no_questions_in_parallel(&workflow.nodes, None, &mut errors);

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

    errors
}

/// What starting a run in `mode` — fresh, not promoted into it — would
/// meet: a read only an earlier mode's work answers, which such a run
/// does not hold. Nothing for a mode the workflow does not declare.
pub fn check_mode_start(workflow: &Workflow, mode: &yunta_core::ModeName) -> Vec<CheckError> {
    if workflow
        .modes
        .as_ref()
        .is_none_or(|modes| !modes.contains_key(mode))
    {
        return Vec::new();
    }
    check_reads(workflow, &Birth::of(workflow, &[]), Some(mode)).errors
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
    let mut warnings = check_reads(workflow, &Birth::of(workflow, &[]), None).warnings;
    collect_parallel_warnings(&workflow.nodes, &mut warnings);
    collect_fanout_warnings(workflow, config, &mut warnings);
    collect_push_to_base_warnings(workflow, config, &mut warnings);
    // A node's `network: false` that its resolved adapter cannot enforce is
    // reported at run time, before the session, as `capability_degraded`
    // (D119) — the record that survives on the log. D119's matching
    // pre-flight warning ("no candidate of this runner declares network
    // isolation") waits for capability-aware `check`: this entry takes no
    // adapter registry, so it cannot yet tell which candidates enforce it.
    warnings
}
