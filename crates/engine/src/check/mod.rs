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
//! refused. An adapter's `sandbox` setting counts the same way: a node
//! asking for the network under one that keeps its sessions read-only is
//! refused, and a session that runs unconfined cannot keep its network
//! closed.
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
mod pointer;
mod programs;
mod refs;
mod runners;
mod scopes;
mod site;
mod sources;
mod unset;

pub use capabilities::AdapterFacts;
pub use context_files::{check_context_files, ContextFilesCheck, RunTreeOrigin};
pub use error::CheckError;
pub use gates::UntakenRoute;
pub(crate) use programs::leading_programs;
pub use programs::programs_named;
pub use refs::{check_workflow_refs, Composition, RefsCheck};
pub use sources::Unanswerable;

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
pub(crate) use yunta_core::caution::{Caution, MissingContextFile, PromotedRead};
pub(crate) use yunta_core::template::template_variables;
pub(crate) use yunta_core::workflow::Place;
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
    declared: &dyn Fn(&yunta_core::AdapterId) -> Option<AdapterFacts>,
) -> Vec<CheckError> {
    check_mounted(workflow, config, declared, &[])
}

/// [`check`] for a run born holding what `mounts` carry in from the
/// workflow that composes it — what a composed workflow's reads may be
/// answered by, and a workflow a person starts never is.
pub fn check_mounted(
    workflow: &Workflow,
    config: &ConfigLayer,
    declared: &dyn Fn(&yunta_core::AdapterId) -> Option<AdapterFacts>,
    mounts: &[yunta_core::MountSpec],
) -> Vec<CheckError> {
    // `read` hands back the expanded graph, and this is asked of a
    // workflow that read — so the shape here is the one a run builds.
    // The clone is for the rules that are about the shape as *written*:
    // a fan-out declaration, a `kind: workflow` node's own, a mount's.
    let mut workflow = workflow.clone();
    let mut errors = Vec::new();
    check_as_written(&workflow, config, &mut errors);
    yunta_core::workflow::read::expand_runner_fanout(&mut workflow);
    yunta_core::workflow::read::expand_implicit_dependencies(&mut workflow);
    let workflow = &workflow;
    check_declarations(workflow, config, declared, mounts, &mut errors);
    check_command_policy(workflow, config, &mut errors);
    check_node_bindings(workflow, config, &mut errors);

    check_no_gate_in_parallel(&workflow.nodes, None, &mut errors);
    check_no_questions_in_parallel(&workflow.nodes, None, &mut errors);

    if let Some(cycle) = find_depends_on_cycle(&workflow.nodes) {
        errors.push(CheckError::DependsOnCycle { cycle });
    }

    check_input_specs(&workflow.inputs, &mut errors);
    check_input_references(workflow, &mut errors);

    errors
}

/// The rules about the workflow as it is written, before its fan-outs and
/// implicit dependencies are expanded into it.
fn check_as_written(workflow: &Workflow, config: &ConfigLayer, errors: &mut Vec<CheckError>) {
    check_runner_fanout(workflow, errors);
    check_workflow_nodes(&workflow.nodes, None, errors);
    check_mounts(workflow, errors);
    check_retired_within(workflow, errors);
    for node in workflow.iter_nodes() {
        check_expansion_mode(node, config, errors);
    }
}

/// The rules about what the workflow's nodes declare and what its reads are
/// answered by: adapters, config keys, artifacts and inputs.
fn check_declarations(
    workflow: &Workflow,
    config: &ConfigLayer,
    declared: &dyn Fn(&yunta_core::AdapterId) -> Option<AdapterFacts>,
    mounts: &[yunta_core::MountSpec],
    errors: &mut Vec<CheckError>,
) {
    // What a node asks of its adapter, checked against what this binary
    // built. `permissions:` and `agent:` have no fallback — refusing
    // here is the whole of that policy.
    capabilities::check_adapter_capabilities(workflow, config, declared, errors);

    check_fanout_scopes(workflow, config, errors);
    check_resume_session(workflow, errors);
    check_ask(workflow, errors);
    check_network(workflow, config, declared, errors);
    check_yunta_schema(workflow, errors);
    check_config_defaults(config, errors);
    check_unset_keys(workflow, config, errors);
    check_config_commands(config, errors);
    check_distill_paths(workflow, errors);
    check_artifact_declarations(workflow, errors);
    check_asking_nodes(workflow, errors);
    check_input_documents(workflow, errors);
    check_reserved_artifact_names(workflow, errors);
    check_answer_sources(workflow, errors);
    errors.extend(check_reads(workflow, &Birth::of(workflow, mounts), None).errors);
    check_named_artifact_sources(workflow, errors);
    check_node_outputs(workflow, errors);
}

/// The project's command rules, applied to the commands the workflow's nodes
/// run and to the hooks its `node_defaults` gives them.
fn check_command_policy(workflow: &Workflow, config: &ConfigLayer, errors: &mut Vec<CheckError>) {
    let Some(permissions) = &config.permissions else {
        return;
    };
    check_commands(&workflow.nodes, config, permissions, errors);
    if let Some(default_hooks) = workflow
        .node_defaults
        .as_ref()
        .and_then(|defaults| defaults.hooks.as_ref())
    {
        // node_defaults hooks run on every node that declares none of
        // its own — their commands are as real as any node's.
        for command in default_hooks
            .before
            .iter()
            .chain(&default_hooks.after)
            .filter_map(|step| step.run.text(config))
        {
            if let Some(denied) = crate::permissions::command_violation(command, Some(permissions))
            {
                errors.push(CheckError::CommandDenied {
                    node: NODE_DEFAULTS.clone(),
                    denied,
                });
            }
        }
    }
}

/// Each node's runner is one the config defines with candidates, its
/// `context:` sits only on a kind that opens a session, and its gate is
/// checked by `check_gate`.
fn check_node_bindings(workflow: &Workflow, config: &ConfigLayer, errors: &mut Vec<CheckError>) {
    for node in &workflow.nodes {
        if let Some(runner) = &node.runner {
            match config.runners.as_ref().and_then(|r| r.get(runner)) {
                None => errors.push(CheckError::UnknownRunner {
                    node: node.id.clone(),
                    runner: runner.clone(),
                    near: near_runner(config, runner),
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

        if !node.context.is_empty() && !node.opens_session() {
            errors.push(CheckError::ContextOnUnsupportedNode {
                node: node.id.clone(),
            });
        }

        check_gate(node, config, errors);
    }
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
///
/// A node's `network: false` that its resolved adapter cannot enforce is
/// reported at run time, ahead of the session, as `capability_degraded` —
/// the record that survives on the log. The adapter-aware warning is the
/// one that judges what each candidate declares, the run's token cap.
pub fn check_warnings(
    workflow: &Workflow,
    config: &ConfigLayer,
    declared: &dyn Fn(&yunta_core::AdapterId) -> Option<AdapterFacts>,
) -> Vec<Caution> {
    let mut warnings = check_reads(workflow, &Birth::of(workflow, &[]), None).warnings;
    collect_parallel_warnings(&workflow.nodes, &mut warnings);
    collect_fanout_warnings(workflow, config, &mut warnings);
    collect_push_to_base_warnings(workflow, config, &mut warnings);
    warnings.extend(
        workflow
            .nodes
            .iter()
            .filter(|node| node.optional && needs_nothing(node))
            .map(|node| Caution::OptionalNeedsNothing {
                node: node.id.clone(),
            }),
    );
    warnings.extend(capabilities::token_cap_cautions(workflow, config, declared));
    warnings
}

/// Whether `node` — its `parallel` children included — needs no key the
/// config could leave unset.
fn needs_nothing(node: &Node) -> bool {
    yunta_core::ConfigKey::needed_by(node).is_empty()
        && match &node.kind {
            NodeKind::Parallel { nodes, .. } => nodes.iter().all(needs_nothing),
            _ => true,
        }
}

/// A loop's declared expansion mode against the merged ceiling. It holds every
/// loop, wherever it sits, a `parallel` child included. An absent block is
/// `deny` — the strictest — so only an explicit, too-permissive declaration
/// can trip.
fn check_expansion_mode(node: &Node, config: &ConfigLayer, errors: &mut Vec<CheckError>) {
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
            if !ceiling.max_mode.allows(se.mode) {
                errors.push(CheckError::ScopeExpansionModeOverCeiling {
                    node: node.id.clone(),
                    mode: se.mode.as_str(),
                    ceiling: ceiling.max_mode.0.as_str(),
                });
            }
        }
    }
}

/// Every loop that still declares `scope_expansion.within`, wherever it
/// sits: nothing bounds an expansion by that key, so a loop keeping it
/// would look bounded when it is not.
fn check_retired_within(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    for node in workflow.iter_nodes() {
        if let NodeKind::Loop {
            scope_expansion: Some(expansion),
            ..
        } = &node.kind
        {
            if expansion.retired_within.is_some() {
                errors.push(CheckError::ScopeExpansionWithinRetired {
                    node: node.id.clone(),
                });
            }
        }
    }
}
