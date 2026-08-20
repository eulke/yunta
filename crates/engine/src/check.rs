//! `yunta check` (T1.3).
//!
//! [`check`] validates one workflow file against the merged config and
//! **never reads other files**: node-id uniqueness (every `parallel`
//! child included), `depends_on` references and acyclicity (I14: never
//! relaxed by `on_failure.goto`, a separate edge set), goto/gate/mode
//! reference integrity, runner resolution, scope disjointness for
//! `parallel` and DAG fan-out (D100/DI-12), permission ceilings over
//! literal commands (§6.1), input specs and references (§2.3),
//! `yunta_schema` (§2.1), and T9.3/T9.4's workflow-node and fan-out
//! declaration rules. [`check_workflow_refs`] is the deliberate
//! exception that does read files: the composition reference graph
//! (`use:` resolves, acyclic, within `limits.max_workflow_depth`, §12)
//! against the repo's `.yunta/workflows/` catalog — a separate entry
//! point so `check`'s no-IO property stays intact, called alongside it
//! by the CLI.
//!
//! Capability-aware checks ("existencia de agentes pedidos", required
//! capabilities) wait for the `Adapter` trait (T3.1) to exist — there is
//! nothing to probe yet.

use std::collections::{HashMap, HashSet};

use thiserror::Error;
use yunta_core::{ConfigLayer, InputSpec, Node, NodeId, NodeKind, Workflow};

use crate::ledger::globs_might_overlap;
use crate::template::template_variables;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckError {
    #[error("duplicate node id `{id}`")]
    DuplicateNodeId { id: NodeId },

    #[error("node `{node}` depends_on unknown node `{unknown}`")]
    UnknownDependency { node: NodeId, unknown: NodeId },

    #[error("node `{node}` on_failure.goto targets unknown node `{target}`")]
    UnknownGotoTarget { node: NodeId, target: NodeId },

    #[error("cycle in depends_on: {path}")]
    DependsOnCycle { path: String },

    #[error("node `{node}` references runner `{runner}`, which `runners:` does not define")]
    UnknownRunner { node: NodeId, runner: String },

    #[error(
        "node `{node}` references runner `{runner}`, which `runners:` defines with zero candidates"
    )]
    RunnerHasNoCandidates { node: NodeId, runner: String },

    /// D100/§5.8: `parallel`'s children share one worktree — a scope
    /// overlap between two of them is a verifiable-in-advance write
    /// collision, error rather than warning.
    #[error(
        "parallel group `{group}`: children `{a}` and `{b}` declare overlapping scope \
         (`{glob_a}` / `{glob_b}`) — they run at once and share one worktree"
    )]
    OverlappingParallelScope {
        group: NodeId,
        a: NodeId,
        b: NodeId,
        glob_a: String,
        glob_b: String,
    },

    /// DI-13: `defaults.on_failure` values beyond `pause` (today's
    /// behavior) have no implementation — refused, never silently read
    /// as `pause`.
    #[error(
        "`defaults.on_failure: {on_failure:?}` is not built yet — only `pause` is; remove \
         the field or declare `pause`"
    )]
    DefaultOnFailureUnsupported {
        on_failure: yunta_core::DefaultOnFailure,
    },

    /// T9.4: `runner:` and `runners:` on one node is a contradiction,
    /// not a merge.
    #[error("node `{node}` declares both `runner:` and `runners:` — use exactly one")]
    BothRunnerAndRunners { node: NodeId },

    /// T9.4: once `review` is many nodes there is no unambiguous
    /// "return control to review" — a re-route, gate `on:` or context
    /// artifact reference must name a specific node.
    #[error(
        "node `{node}` targets `{target}`, which is a `runners:` fan-out — target one of its \
         expanded nodes (`{target}@<role>`) or a non-fan-out node"
    )]
    FanOutTarget { node: NodeId, target: NodeId },

    /// DI-24: `on_finish.distill` names a path no node declares
    /// producing — statically wrong (the runtime "declared but not
    /// produced this run" case degrades to a finding instead).
    #[error(
        "`on_finish.distill` names `{path}` but no node's `artifacts.produces` declares it — \
         declare the artifact on the node that writes it, or drop it from `distill`"
    )]
    DistillUnknownArtifact { path: String },

    /// DI-13: `fresh_context: false` requires session resume (DI-23),
    /// which isn't built — refused up front instead of accepted and
    /// silently ignored (A6).
    #[error(
        "node `{node}` declares `fresh_context: false` but session resume is not supported \
         yet — remove the field (every session is fresh today) or wait for `resume_session`"
    )]
    FreshContextUnsupported { node: NodeId },

    /// DI-13/§2.1: the workflow demands a schema this binary doesn't
    /// speak, or a range the parser can't read.
    #[error("`yunta_schema: \"{range}\"` — {detail} (this binary speaks schema {binary})")]
    YuntaSchemaMismatch {
        range: String,
        detail: String,
        binary: u32,
    },

    /// DI-12: D100 extended to the DAG's *implicit* fan-out — two
    /// top-level nodes with no dependency path between them can be
    /// `ready` together, and with `max_parallel_nodes > 1` they share
    /// one worktree at once, exactly the physical risk `parallel`
    /// already errors on. Static approximation by design: "no relative
    /// order declared" is the rule, never a simulation of what the
    /// scheduler would actually interleave.
    #[error(
        "nodes `{a}` and `{b}` have no dependency path between them and declare overlapping \
         scope (`{glob_a}` / `{glob_b}`) — with `max_parallel_nodes` > 1 they can write the \
         same paths at once (D100); chain them with `depends_on` or make their scopes disjoint"
    )]
    OverlappingFanOutScope {
        a: NodeId,
        b: NodeId,
        glob_a: String,
        glob_b: String,
    },

    /// §6.1's first enforcement moment (T5.7): the command as written in
    /// the YAML already violates the merged `permissions` model. The scan
    /// matches the *literal* text — a command assembled by template gets
    /// caught by the second moment, at runtime, right before execution.
    #[error("node `{node}`: {rule}")]
    CommandDenied { node: NodeId, rule: String },

    /// §9/T6.1/DI-17: `context:` is resolved into a session's own
    /// prompt — `kind: prompt` (the node's one session) and `kind:
    /// loop` (once per task brief) are the kinds that open one; a
    /// `bash`/`check`/`executor`/`gate` node has no session to consume
    /// it. Declaring it there is caught here rather than silently
    /// ignored at runtime (A6).
    #[error(
        "node `{node}`: `context:` is only supported on `kind: prompt` and `kind: loop` nodes \
         — nothing else opens a session that could consume it"
    )]
    ContextOnUnsupportedNode { node: NodeId },

    /// T1.5/§2.3/D82: the two fields are mutually exclusive by
    /// definition — a `default` is what makes an input optional at all.
    #[error(
        "input `{name}` declares both `required: true` and a `default` — \
         §2.3 makes them mutually exclusive"
    )]
    InputRequiredWithDefault { name: String },

    /// The mirror case: `required: false` with nothing to fall back to
    /// would resolve to no value at all, which no `{{inputs.x}}` render
    /// site can represent.
    #[error(
        "input `{name}` declares `required: false` with no `default` — \
         give it a default, or drop `required: false` (the implicit default when neither is given)"
    )]
    InputOptionalWithoutDefault { name: String },

    #[error("input `{name}` is type `enum` with an empty `values` list")]
    InputEmptyEnumValues { name: String },

    #[error("input `{name}`'s `min` ({min}) is greater than its `max` ({max})")]
    InputMinExceedsMax {
        name: String,
        min: String,
        max: String,
    },

    #[error("input `{name}`'s `pattern` `{pattern}` is not a valid regex: {detail}")]
    InputInvalidPattern {
        name: String,
        pattern: String,
        detail: String,
    },

    /// §2.3: "`check` verifica que todo `{{{{inputs.x}}}}` refiera a un
    /// input declarado" — scanned wherever a template can appear inline
    /// in the workflow (prompt text, `bash`/hook commands, `context:`
    /// patterns and command/query text). A `prompt: {file: ...}` body
    /// isn't scanned: `check` never reads files (see this module's own
    /// doc comment), so an undeclared reference there still only
    /// surfaces at run time, same as it did before T1.5.
    #[error("node `{node}` references `{{{{inputs.{name}}}}}`, which `inputs:` does not declare")]
    UndeclaredInput { node: NodeId, name: String },

    /// §5.6/D66/T7.7: a `kind: gate` with `external:` has nowhere to
    /// actually publish without a forge — refused here rather than at
    /// runtime (A6), the same "check catches what a run would only
    /// discover after spending something" reasoning `UnknownRunner`
    /// already applies. This checks only that a forge is *configured*
    /// — a specific machine lacking the named credential env var at
    /// *runtime* is a different, degrade-not-refuse case (D66's own
    /// "sin credenciales... degrada a consola").
    #[error("node `{node}`: `kind: gate` with `external: {{kind: pull_request}}` needs `forge.github` configured")]
    ExternalGateWithoutForge { node: NodeId },

    /// A gate's resolution is a forge round-trip, one at a time — never
    /// scoped to a `parallel` group's shared worktree/join semantics
    /// (T7.7 doesn't define what either would mean for a gate).
    #[error("node `{node}`: `kind: gate` can't be a `parallel` child (group `{group}`)")]
    GateInsideParallel { node: NodeId, group: NodeId },

    /// DI-04: `on:` may only map options the gate itself declares —
    /// mapping an undeclared one is a choice no human can ever make.
    #[error("gate `{node}`: `on.{option}` maps an option `options:` does not declare")]
    GateOnUndeclaredOption { node: NodeId, option: String },

    /// DI-04: same broken-reference class as `UnknownGotoTarget`, for a
    /// gate option's re-route target.
    #[error("gate `{node}`: `on.{option}` targets unknown node `{target}`")]
    UnknownGateOptionTarget {
        node: NodeId,
        option: String,
        target: NodeId,
    },

    /// §10.1/D44: same broken-reference class as `UnknownGotoTarget` —
    /// catching it here means the run never starts with a mode that
    /// silently omits work its own author meant to include.
    #[error("mode `{mode}` includes unknown node `{node}`")]
    ModeReferencesUnknownNode { mode: String, node: NodeId },

    /// §10.1: "un modo recorta deliberación, jamás verificación" — checked
    /// independent of the mode's name or count, exactly D44's own text.
    #[error("node `{node}` is `invariant: true` but mode `{mode}` doesn't include it")]
    InvariantNodeExcludedFromMode { node: NodeId, mode: String },

    /// §10.1's own coherence rule, and its own reasoning for making it an
    /// error rather than a warning: the same broken-goto class
    /// `UnknownGotoTarget` catches, just scoped to one mode's variant of
    /// the graph instead of the whole file. The message names both ways
    /// out, per §10.1's own text ("nombra las dos salidas posibles").
    #[error(
        "node `{node}` is in mode `{mode}`, but its on_failure.goto target `{goto}` isn't — \
         include `{goto}` in `{mode}`, or drop the re-route there"
    )]
    RerouteTargetExcludedFromMode {
        mode: String,
        node: NodeId,
        goto: NodeId,
    },

    /// T9.3: a `kind: workflow` node never opens a session of its own —
    /// the child's nodes bind their own runners — so a runner binding
    /// here would be accepted and ignored, exactly what A6 forbids.
    #[error(
        "node `{node}`: `{field}` has no meaning on `kind: workflow` — the child workflow's \
         own nodes bind their runners"
    )]
    WorkflowNodeRunnerBinding { node: NodeId, field: &'static str },

    /// §12: "hijos paralelos con `inherit` exigen scopes disjuntos,
    /// validado en check" — an `inherit` child shares the parent's one
    /// tree with every concurrent sibling, so an undeclared scope makes
    /// disjointness unverifiable: refused, same rank as
    /// `OverlappingParallelScope` (which catches the declared-overlap
    /// half of the same rule).
    #[error(
        "parallel group `{group}`: child `{node}` is `kind: workflow` with `isolation: \
         inherit` and no `scope` — inherit children share the parent's tree, so each must \
         declare a disjoint scope (§12)"
    )]
    InheritChildWithoutScope { group: NodeId, node: NodeId },

    /// D108: a mount reads a node of the parent's own graph — an
    /// unknown name is the same broken-reference class as
    /// `UnknownDependency`, named for the field the author actually
    /// wrote.
    #[error(
        "node `{node}`: `mounts` references node `{target}` which this workflow does not \
         define — name a node of this same workflow (§12/D108)"
    )]
    MountUnknownNode { node: NodeId, target: NodeId },

    /// D108: mounting one's own artifact is a read of an outcome that
    /// cannot exist yet — the implied `depends_on` would be a self-cycle.
    #[error(
        "node `{node}`: `mounts` references the node itself — a mount reads a *finished* \
         node's artifact, which this node cannot be for its own birth (§12/D108)"
    )]
    MountOnSelf { node: NodeId },

    /// D108: same reasoning as `FanOutTarget` — once `runners:` expands
    /// a node into `<id>@<role>` siblings there is no "the" node to
    /// mount from.
    #[error(
        "node `{node}`: `mounts` references `{target}`, which `runners:` fans out into one \
         node per role — mount a specific `{target}@<role>` sibling instead (§13.2/D108)"
    )]
    MountOnFanOut { node: NodeId, target: NodeId },

    /// D108: parallel children run concurrently — no DAG order exists
    /// inside the group, so §12's "hermanos terminados" cannot hold
    /// there and the implied `depends_on` would mean nothing.
    #[error(
        "parallel group `{group}`: child `{node}` declares `mounts` — parallel children have \
         no order to guarantee a finished source; mount on a top-level workflow node instead \
         (§12/D108)"
    )]
    MountInsideParallel { group: NodeId, node: NodeId },

    /// DI-23/D99: only `kind: prompt` opens a node-scoped session —
    /// declaring `resume_session` anywhere else names a conversation
    /// that doesn't exist (a loop's per-task sessions re-run from the
    /// ledger; bash/check/executor/gate/workflow open none).
    #[error(
        "node `{node}`: `on_interrupt: resume_session` is only supported on `kind: prompt`          nodes — nothing else has a node-scoped session to resume; declare `restart_node`          (the default) instead"
    )]
    ResumeSessionOnSessionlessNode { node: NodeId },

    /// DI-20/§6.2: the node asks for a scope-expansion mode more
    /// permissive than the merged `permissions.scope_expansion.max_mode`
    /// ceiling allows — same only-narrowing model as every other
    /// `permissions` group (§6.1); which *layer* set the binding ceiling
    /// is `permission_layer_conflicts`' territory at config load.
    #[error(
        "node `{node}`: `scope_expansion.mode: {mode}` exceeds the merged permissions ceiling \
         `scope_expansion.max_mode: {ceiling}` (§6.2/D73) — harden the node's mode, or raise \
         the ceiling in the layer that set it"
    )]
    ScopeExpansionModeOverCeiling {
        node: NodeId,
        mode: &'static str,
        ceiling: &'static str,
    },

    /// DI-18: a 0 would starve every ready node forever — a config
    /// mistake surfaced here as a refusal (the scheduler's clamp to 1
    /// stays as defense in depth for runs created before this rule).
    #[error(
        "`defaults.max_parallel_nodes: 0` would starve every node forever — declare 1 or more, \
         or drop the field (default: 1)"
    )]
    MaxParallelNodesZero,

    /// T9.3: a composition reference that can't resolve today — the
    /// same broken-reference class as `UnknownGotoTarget`, across
    /// files. Advisory about the *current* catalog by design: the child
    /// freezes its own file at birth, so a run only ever meets the file
    /// as it is then.
    #[error(
        "node `{node}`: `use: {name}` cannot be read from the repo catalog `{path}` — add \
         the workflow file (versioned) or fix the name"
    )]
    WorkflowRefMissing {
        node: NodeId,
        name: String,
        path: std::path::PathBuf,
    },

    #[error("workflow `{path}` (referenced through composition) does not parse: {detail}")]
    WorkflowRefUnparseable {
        path: std::path::PathBuf,
        detail: String,
    },

    /// §12: "el grafo de referencias entre workflows sea acíclico".
    #[error("workflow composition cycle: {chain}")]
    WorkflowRefCycle { chain: String },

    /// §12's "profundidad máxima configurable", checked statically over
    /// the reference graph (the runtime guard at child birth enforces
    /// the same limit over what actually loads).
    #[error(
        "workflow composition {chain} nests {depth} level(s) deep but \
         `limits.max_workflow_depth` is {max} — flatten the composition or raise the limit"
    )]
    WorkflowRefTooDeep { chain: String, depth: u32, max: u32 },
}

/// A non-blocking finding — the run can still start (D100/§5.8: `check`
/// warns, it doesn't refuse, when a collision can't be verified for lack
/// of declared scope). Kept separate from `CheckError` rather than adding
/// a severity field to it: every existing caller of `check()` keeps
/// treating its `Vec<CheckError>` as "must be empty to proceed" without
/// learning to filter by severity.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckWarning {
    #[error(
        "parallel group `{group}`: two or more children can write and don't declare scope as \
         disjoint — the engine can't verify they won't collide (D100); declare `scope` on each \
         to make the check real"
    )]
    UndeclaredParallelScope { group: NodeId },

    /// DI-12: the fan-out analogue of `UndeclaredParallelScope` — one
    /// warning per connected component of mutually-independent,
    /// write-capable, scope-less top-level nodes (per pair would drown
    /// the signal in noise).
    #[error(
        "nodes {nodes} have no dependency paths between them and can all write without \
         declared scope — with `max_parallel_nodes` > 1 the engine can't verify they won't \
         collide (D100); declare `scope` on each or chain them with `depends_on`"
    )]
    UndeclaredFanOutScope { nodes: String },

    /// DI-18/D48: a literal `git push` aimed at the base branch with no
    /// gate anywhere before it in the DAG — warning, not error (D48's
    /// own rank): a team may genuinely want it, but nobody should
    /// discover an ungated push to `main` from the push itself.
    #[error(
        "node `{node}` pushes to the base branch (`{branch}`) with no gate anywhere before it \
         in the DAG — D48: put a gate ahead of the push, or push to `{{{{run.branch}}}}`"
    )]
    PushToBaseWithoutGate { node: NodeId, branch: String },
}

/// Validates a workflow against the M-0 rule set. Every applicable rule is
/// checked and every violation reported — not just the first one (same
/// spirit as the ledger's T1.0 §4: whoever writes this by hand corrects
/// once, not once per `yunta check` run).
pub fn check(workflow: &Workflow, config: &ConfigLayer) -> Vec<CheckError> {
    // §9: `context: [{ artifact }]` creates an implicit `depends_on` edge
    // — expanded here, on this function's own clone, so cycle detection
    // below sees exactly the graph a real run would build (`build_manifest`
    // expands the same way), never a narrower one that misses a cycle
    // formed only through context references.
    let mut workflow = workflow.clone();
    let mut errors = Vec::new();
    // T9.4: fan-out declarations validate on the *original* shape (the
    // rules are about the declaration itself), then the graph expands so
    // every later rule sees what will actually run.
    check_runner_fanout(&workflow, &mut errors);
    // T9.3: workflow-node rules also validate the original shape
    // (`runners:` on one is refused before expansion would multiply it).
    check_workflow_nodes(&workflow.nodes, None, &mut errors);
    // D108: mount declarations too — expansion below turns each mount
    // into an ordinary `depends_on` edge, so cycle detection sees them.
    check_mounts(&workflow, &mut errors);
    crate::manifest::expand_runner_fanout(&mut workflow);
    crate::manifest::expand_implicit_dependencies(&mut workflow);
    let workflow = &workflow;

    // Global, not per-group: replay derives node state from one flat
    // NodeId -> NodeState map (I2), so a `parallel` child's id colliding
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
    check_fresh_context(workflow, &mut errors);
    check_yunta_schema(workflow, &mut errors);
    check_config_defaults(config, &mut errors);
    check_distill_paths(workflow, &mut errors);

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
                        node: "node_defaults".into(),
                        rule,
                    });
                }
            }
        }
    }

    for node in &workflow.nodes {
        for dep in &node.depends_on {
            if !known_ids.contains(dep) {
                errors.push(CheckError::UnknownDependency {
                    node: node.id.clone(),
                    unknown: dep.clone(),
                });
            }
        }

        if let Some(on_failure) = &node.on_failure {
            if !known_ids.contains(&on_failure.goto) {
                errors.push(CheckError::UnknownGotoTarget {
                    node: node.id.clone(),
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

        // DI-20/§6.2: the loop's declared expansion mode against the
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

/// §10.1/D44: `modes:`'s own three invariants — independent of the
/// mode's name or count, checked once per declared mode. `include: all`
/// is trivially coherent (everything's in it), so only the explicit
/// node-list form has anything to check.
fn check_modes(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let Some(modes) = &workflow.modes else {
        return;
    };

    // Mode `include:` only ever names *top-level* nodes (§10.1's own
    // examples never reach into a `parallel` group's children) — a
    // `parallel` group is included or excluded as a whole, so
    // "known" here deliberately excludes nested child ids even though
    // `check`'s other rules track them for global uniqueness.
    let top_level_ids: HashSet<&NodeId> = workflow.nodes.iter().map(|n| &n.id).collect();
    let invariant_ids: Vec<&NodeId> = workflow
        .nodes
        .iter()
        .filter(|n| n.invariant)
        .map(|n| &n.id)
        .collect();

    for (mode_name, spec) in modes {
        let yunta_core::ModeInclude::Nodes(included_ids) = &spec.include else {
            continue; // `all` — every invariant below is vacuously true
        };
        for id in included_ids {
            if !top_level_ids.contains(id) {
                errors.push(CheckError::ModeReferencesUnknownNode {
                    mode: mode_name.clone(),
                    node: id.clone(),
                });
            }
        }

        let included: HashSet<&NodeId> = included_ids.iter().collect();
        for invariant_id in &invariant_ids {
            if !included.contains(invariant_id) {
                errors.push(CheckError::InvariantNodeExcludedFromMode {
                    node: (*invariant_id).clone(),
                    mode: mode_name.clone(),
                });
            }
        }

        for node in workflow.nodes.iter().filter(|n| included.contains(&n.id)) {
            if let Some(on_failure) = &node.on_failure {
                if !included.contains(&on_failure.goto) {
                    errors.push(CheckError::RerouteTargetExcludedFromMode {
                        mode: mode_name.clone(),
                        node: node.id.clone(),
                        goto: on_failure.goto.clone(),
                    });
                }
            }
            // T1.3's own full wording: "...cuyo `goto` u **opción de
            // gate** apunta a un nodo excluido" — a gate's `on:` target
            // is the same broken-reference class as a re-route's.
            if let NodeKind::Gate { on, .. } = &node.kind {
                for target in on.values() {
                    if !included.contains(target) {
                        errors.push(CheckError::RerouteTargetExcludedFromMode {
                            mode: mode_name.clone(),
                            node: node.id.clone(),
                            goto: target.clone(),
                        });
                    }
                }
            }
        }
    }
}

/// T1.5/§2.3: each declared input's own fields are internally consistent
/// — independent of anything else in the workflow, so this runs once
/// over `inputs:` rather than per reference site.
fn check_input_specs(
    inputs: &std::collections::BTreeMap<String, InputSpec>,
    errors: &mut Vec<CheckError>,
) {
    for (name, spec) in inputs {
        let has_default = spec.has_default();
        match spec.required_field() {
            Some(true) if has_default => {
                errors.push(CheckError::InputRequiredWithDefault { name: name.clone() });
            }
            Some(false) if !has_default => {
                errors.push(CheckError::InputOptionalWithoutDefault { name: name.clone() });
            }
            _ => {}
        }

        match spec {
            InputSpec::Enum { values, .. } if values.is_empty() => {
                errors.push(CheckError::InputEmptyEnumValues { name: name.clone() });
            }
            InputSpec::Number {
                min: Some(min),
                max: Some(max),
                ..
            } if min > max => {
                errors.push(CheckError::InputMinExceedsMax {
                    name: name.clone(),
                    min: min.to_string(),
                    max: max.to_string(),
                });
            }
            InputSpec::String {
                pattern: Some(pattern),
                ..
            } => {
                if let Err(e) = regex::Regex::new(pattern) {
                    errors.push(CheckError::InputInvalidPattern {
                        name: name.clone(),
                        pattern: pattern.clone(),
                        detail: e.to_string(),
                    });
                }
            }
            _ => {}
        }
    }
}

/// §2.3: every `{{inputs.x}}` appearing in an inline template must name
/// a declared input. Scans exactly the text this recorte's runtime ever
/// renders (`node_exec.rs`/`context_resolve.rs`'s own `render_template`
/// call sites) — prompt text, bash/hook commands, and `context:`
/// patterns/command/query — so a reference `check` accepts is guaranteed
/// renderable and vice versa.
fn check_input_references(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    if let Some(defaults) = &workflow.node_defaults {
        if let Some(hooks) = &defaults.hooks {
            for step in hooks.before.iter().chain(&hooks.after) {
                check_template_text(&"node_defaults".into(), &step.run, workflow, errors);
            }
        }
    }
    check_input_references_in_nodes(&workflow.nodes, workflow, errors);
}

fn check_input_references_in_nodes(
    nodes: &[Node],
    workflow: &Workflow,
    errors: &mut Vec<CheckError>,
) {
    for node in nodes {
        match &node.kind {
            NodeKind::Prompt {
                prompt: yunta_core::PromptSource::Inline(text),
            } => check_template_text(&node.id, text, workflow, errors),
            NodeKind::Bash { run } => check_template_text(&node.id, run, workflow, errors),
            NodeKind::Loop { until, prompt, .. } => {
                check_template_text(&node.id, until, workflow, errors);
                if let yunta_core::PromptSource::Inline(text) = prompt {
                    check_template_text(&node.id, text, workflow, errors);
                }
            }
            NodeKind::Parallel {
                nodes: children, ..
            } => {
                check_input_references_in_nodes(children, workflow, errors);
            }
            _ => {}
        }

        if let Some(hooks) = &node.hooks {
            for step in hooks.before.iter().chain(&hooks.after) {
                check_template_text(&node.id, &step.run, workflow, errors);
            }
        }

        for source in &node.context {
            match source {
                yunta_core::ContextSpec::Files { files } => {
                    for pattern in files {
                        check_template_text(&node.id, pattern, workflow, errors);
                    }
                }
                yunta_core::ContextSpec::Command { command } => {
                    check_template_text(&node.id, command, workflow, errors);
                }
                yunta_core::ContextSpec::Mcp { mcp } => {
                    check_template_text(&node.id, &mcp.query, workflow, errors);
                }
                _ => {}
            }
        }
    }
}

fn check_template_text(
    node: &NodeId,
    text: &str,
    workflow: &Workflow,
    errors: &mut Vec<CheckError>,
) {
    let Ok(variables) = template_variables(text) else {
        // An unclosed `{{` is a template-syntax error, not an inputs
        // one — the runtime's own `render_template` reports that when
        // this node actually executes; nothing new to say here.
        return;
    };
    for variable in variables {
        if let Some(name) = variable.strip_prefix("inputs.") {
            if !workflow.inputs.contains_key(name) {
                errors.push(CheckError::UndeclaredInput {
                    node: node.clone(),
                    name: name.to_string(),
                });
            }
        }
    }
}

/// Non-blocking findings — D100/§5.8's "can't verify, so warn" case.
/// Separate entry point from [`check`] rather than a severity field on
/// `CheckError`, so nothing that already treats `check()`'s output as
/// "must be empty to proceed" has to learn to filter by severity.
///
/// D100's real condition is "two or more children **with write
/// permissions**" — and since T5.7 gave nodes `permissions:
/// read-only|edit|full`, a child declaring `read-only` is out of the
/// collision count by declaration. A child without the field stays
/// implicitly write-capable (the engine's default profile is `edit`).
pub fn check_warnings(workflow: &Workflow, config: &ConfigLayer) -> Vec<CheckWarning> {
    let mut warnings = Vec::new();
    collect_parallel_warnings(&workflow.nodes, &mut warnings);
    collect_fanout_warnings(workflow, config, &mut warnings);
    collect_push_to_base_warnings(workflow, config, &mut warnings);
    warnings
}

/// DI-18/D48: scans every literal `bash`/hook command for a `git push`
/// aimed at the base branch — the `{{project.base_branch}}` template,
/// or the configured literal name as its own token (whitespace/refspec
/// boundaries, so a branch named `main` never matches `domain`) — and
/// warns unless a gate sits somewhere before the node in the DAG
/// (transitive `depends_on`; a `parallel` child inherits its group's
/// ancestry). Literal text only, same stance as `check_commands`: a
/// command assembled at runtime is the runtime moment's problem.
fn collect_push_to_base_warnings(
    workflow: &Workflow,
    config: &ConfigLayer,
    warnings: &mut Vec<CheckWarning>,
) {
    let base_branch = config
        .project
        .as_ref()
        .and_then(|project| project.base_branch.as_deref());
    let pushes_to_base = |command: &str| -> Option<String> {
        if !command.contains("git push") {
            return None;
        }
        if command.contains("{{project.base_branch}}") {
            return Some(
                base_branch
                    .map(str::to_string)
                    .unwrap_or_else(|| "{{project.base_branch}}".to_string()),
            );
        }
        let base = base_branch?;
        let named = command
            .split_whitespace()
            .flat_map(|token| token.split(':'))
            .any(|token| token == base);
        named.then(|| base.to_string())
    };

    // Which top-level nodes have a gate somewhere in their transitive
    // `depends_on` ancestry.
    let nodes = &workflow.nodes;
    let index_of: HashMap<&NodeId, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (&node.id, i))
        .collect();
    fn gate_protected(
        i: usize,
        nodes: &[Node],
        index_of: &HashMap<&NodeId, usize>,
        cache: &mut Vec<Option<bool>>,
    ) -> bool {
        if let Some(known) = cache[i] {
            return known;
        }
        cache[i] = Some(false); // cycle guard; real cycles error elsewhere
        let protected = nodes[i].depends_on.iter().any(|dep| {
            index_of.get(dep).is_some_and(|&d| {
                matches!(nodes[d].kind, NodeKind::Gate { .. })
                    || gate_protected(d, nodes, index_of, cache)
            })
        });
        cache[i] = Some(protected);
        protected
    }
    let mut cache: Vec<Option<bool>> = vec![None; nodes.len()];

    for (i, node) in nodes.iter().enumerate() {
        let protected = gate_protected(i, nodes, &index_of, &mut cache);
        // A parallel child's commands push from the same ancestry as
        // its group.
        let mut targets: Vec<(&Node, &str)> = Vec::new();
        fn collect_commands<'a>(node: &'a Node, targets: &mut Vec<(&'a Node, &'a str)>) {
            if let NodeKind::Bash { run } = &node.kind {
                targets.push((node, run));
            }
            if let Some(hooks) = &node.hooks {
                for step in hooks.before.iter().chain(&hooks.after) {
                    targets.push((node, &step.run));
                }
            }
            if let NodeKind::Parallel {
                nodes: children, ..
            } = &node.kind
            {
                for child in children {
                    collect_commands(child, targets);
                }
            }
        }
        collect_commands(node, &mut targets);
        for (owner, command) in targets {
            if let Some(branch) = pushes_to_base(command) {
                if !protected {
                    warnings.push(CheckWarning::PushToBaseWithoutGate {
                        node: owner.id.clone(),
                        branch,
                    });
                }
            }
        }
    }
}

/// Both halves of DI-12 need the same question answered: which pairs of
/// top-level nodes have no dependency path between them in either
/// direction (transitive closure of `depends_on`, with a dependency on
/// a `parallel` child counting as one on its enclosing group)?
fn independent_top_level_pairs(workflow: &Workflow) -> Vec<(usize, usize)> {
    let nodes = &workflow.nodes;
    // Any id (child of a group included) → the top-level index it
    // belongs to.
    let mut owner: std::collections::HashMap<&NodeId, usize> = std::collections::HashMap::new();
    fn claim<'a>(
        node: &'a Node,
        top: usize,
        owner: &mut std::collections::HashMap<&'a NodeId, usize>,
    ) {
        owner.insert(&node.id, top);
        if let NodeKind::Parallel { nodes, .. } = &node.kind {
            for child in nodes {
                claim(child, top, owner);
            }
        }
    }
    for (i, node) in nodes.iter().enumerate() {
        claim(node, i, &mut owner);
    }

    // reachable[i] = every top-level index i transitively depends on.
    let mut reachable: Vec<std::collections::HashSet<usize>> =
        vec![Default::default(); nodes.len()];
    fn walk(
        i: usize,
        nodes: &[Node],
        owner: &std::collections::HashMap<&NodeId, usize>,
        reachable: &mut Vec<std::collections::HashSet<usize>>,
        visiting: &mut Vec<bool>,
    ) {
        if visiting[i] || !reachable[i].is_empty() {
            return;
        }
        visiting[i] = true;
        let deps: Vec<usize> = nodes[i]
            .depends_on
            .iter()
            .filter_map(|dep| owner.get(dep).copied())
            .collect();
        for dep in deps {
            if dep == i {
                continue;
            }
            walk(dep, nodes, owner, reachable, visiting);
            let transitively: Vec<usize> = reachable[dep].iter().copied().collect();
            reachable[i].insert(dep);
            reachable[i].extend(transitively);
        }
        visiting[i] = false;
    }
    let mut visiting = vec![false; nodes.len()];
    for i in 0..nodes.len() {
        walk(i, nodes, &owner, &mut reachable, &mut visiting);
    }

    let mut pairs = Vec::new();
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            if !reachable[i].contains(&j) && !reachable[j].contains(&i) {
                pairs.push((i, j));
            }
        }
    }
    pairs
}

/// A top-level node that can write the shared worktree (same rule as
/// `parallel`'s children, D100): anything not declared `read-only`.
fn writes(node: &Node) -> bool {
    node.permissions != Some(yunta_core::NodePermissions::ReadOnly)
}

/// DI-12's error half: overlapping *declared* scope on an unordered
/// pair — verifiable in advance, so an error, same rank as `parallel`.
fn check_fanout_scopes(workflow: &Workflow, config: &ConfigLayer, errors: &mut Vec<CheckError>) {
    if config.resolved_max_parallel_nodes() <= 1 {
        // Sequential scheduling: successive writes to one worktree are
        // legitimate, there is no concurrency to collide under.
        return;
    }
    for (i, j) in independent_top_level_pairs(workflow) {
        let (a, b) = (&workflow.nodes[i], &workflow.nodes[j]);
        if !(writes(a) && writes(b)) {
            continue;
        }
        for glob_a in &a.scope {
            for glob_b in &b.scope {
                if globs_might_overlap(glob_a, glob_b) {
                    errors.push(CheckError::OverlappingFanOutScope {
                        a: a.id.clone(),
                        b: b.id.clone(),
                        glob_a: glob_a.clone(),
                        glob_b: glob_b.clone(),
                    });
                }
            }
        }
    }
}

/// DI-12's warning half: connected components of mutually-independent,
/// write-capable, scope-less top-level nodes — one warning per
/// component, members named.
fn collect_fanout_warnings(
    workflow: &Workflow,
    config: &ConfigLayer,
    warnings: &mut Vec<CheckWarning>,
) {
    if config.resolved_max_parallel_nodes() <= 1 {
        return;
    }
    let nodes = &workflow.nodes;
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let eligible: Vec<bool> = nodes
        .iter()
        .map(|node| writes(node) && node.scope.is_empty())
        .collect();
    for (i, j) in independent_top_level_pairs(workflow) {
        if eligible[i] && eligible[j] {
            adjacency[i].push(j);
            adjacency[j].push(i);
        }
    }
    let mut seen = vec![false; nodes.len()];
    for start in 0..nodes.len() {
        if seen[start] || adjacency[start].is_empty() {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(i) = stack.pop() {
            if seen[i] {
                continue;
            }
            seen[i] = true;
            component.push(nodes[i].id.clone());
            stack.extend(adjacency[i].iter().copied());
        }
        component.sort();
        warnings.push(CheckWarning::UndeclaredFanOutScope {
            nodes: component
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
}

/// One `parallel` group's scope-collision status (D100): every pair of
/// children whose declared scopes might overlap — computed in one place
/// so `check`'s error and `check_warnings`' warning can never disagree
/// about what overlaps.
struct GroupScope<'a> {
    overlaps: Vec<(&'a Node, &'a Node, &'a str, &'a str)>,
}

fn evaluate_group_scope(children: &[Node]) -> GroupScope<'_> {
    let mut overlaps = Vec::new();
    for i in 0..children.len() {
        for j in (i + 1)..children.len() {
            let (a, b) = (&children[i], &children[j]);
            for glob_a in &a.scope {
                for glob_b in &b.scope {
                    if globs_might_overlap(glob_a, glob_b) {
                        overlaps.push((a, b, glob_a.as_str(), glob_b.as_str()));
                    }
                }
            }
        }
    }
    GroupScope { overlaps }
}

/// Static half of §6.1's runtime rule: every literal command in the
/// workflow — bash `run`, hook steps — against the merged model, parallel
/// children included. Criteria live in the runtime ledger and executors
/// resolve through config, so both are runtime-moment territory.
fn check_commands(
    nodes: &[Node],
    permissions: &yunta_core::PermissionsConfig,
    errors: &mut Vec<CheckError>,
) {
    for node in nodes {
        let mut commands: Vec<&str> = Vec::new();
        if let NodeKind::Bash { run } = &node.kind {
            commands.push(run);
        }
        if let Some(hooks) = &node.hooks {
            commands.extend(
                hooks
                    .before
                    .iter()
                    .chain(&hooks.after)
                    .map(|s| s.run.as_str()),
            );
        }
        for command in commands {
            if let Some(rule) = crate::permissions::command_violation(command, Some(permissions)) {
                errors.push(CheckError::CommandDenied {
                    node: node.id.clone(),
                    rule,
                });
            }
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            check_commands(children, permissions, errors);
        }
    }
}

fn check_parallel_scopes(nodes: &[Node], errors: &mut Vec<CheckError>) {
    for node in nodes {
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            let group = evaluate_group_scope(children);
            for (a, b, glob_a, glob_b) in group.overlaps {
                errors.push(CheckError::OverlappingParallelScope {
                    group: node.id.clone(),
                    a: a.id.clone(),
                    b: b.id.clone(),
                    glob_a: glob_a.to_string(),
                    glob_b: glob_b.to_string(),
                });
            }
            check_parallel_scopes(children, errors);
        }
    }
}

/// §5.6/D66/T7.7 + DI-04: a `kind: gate` with `external:` needs
/// `forge.github` configured (`external.kind` is a closed enum with one
/// variant today, so this is a total match); an internal gate's own
/// `on:` mapping must reference declared options and existing targets —
/// the same broken-reference class `UnknownGotoTarget` already catches.
fn check_gate(
    node: &Node,
    known_ids: &HashSet<NodeId>,
    config: &yunta_core::ConfigLayer,
    errors: &mut Vec<CheckError>,
) {
    let NodeKind::Gate {
        options,
        on,
        external,
        ..
    } = &node.kind
    else {
        return;
    };
    if let Some(external) = external {
        match external.kind {
            yunta_core::ForgeKind::PullRequest => {
                let configured = config
                    .forge
                    .as_ref()
                    .is_some_and(|forge| forge.github.is_some());
                if !configured {
                    errors.push(CheckError::ExternalGateWithoutForge {
                        node: node.id.clone(),
                    });
                }
            }
        }
    }
    for (option, target) in on {
        if !options.iter().any(|declared| declared == option) {
            errors.push(CheckError::GateOnUndeclaredOption {
                node: node.id.clone(),
                option: option.clone(),
            });
        }
        if !known_ids.contains(target) {
            errors.push(CheckError::UnknownGateOptionTarget {
                node: node.id.clone(),
                option: option.clone(),
                target: target.clone(),
            });
        }
    }
}

/// §5.8/T4.6 vs. T7.7: a `parallel` group's children share a worktree
/// and join semantics a forge round-trip has no defined relationship to
/// — refused outright rather than guessing one.
fn check_no_gate_in_parallel(
    nodes: &[Node],
    parent_group: Option<&Node>,
    errors: &mut Vec<CheckError>,
) {
    for node in nodes {
        if let Some(group) = parent_group {
            if matches!(node.kind, NodeKind::Gate { .. }) {
                errors.push(CheckError::GateInsideParallel {
                    node: node.id.clone(),
                    group: group.id.clone(),
                });
            }
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            check_no_gate_in_parallel(children, Some(node), errors);
        }
    }
}

fn collect_parallel_warnings(nodes: &[Node], warnings: &mut Vec<CheckWarning>) {
    for node in nodes {
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            // Only write-capable children can collide (D100): a child
            // declaring `permissions: read-only` is out by declaration.
            let writers: Vec<&Node> = children
                .iter()
                .filter(|child| child.permissions != Some(yunta_core::NodePermissions::ReadOnly))
                .collect();
            if writers.len() >= 2 {
                let group = evaluate_group_scope(children);
                let all_writers_declared = writers.iter().all(|child| !child.scope.is_empty());
                if group.overlaps.is_empty() && !all_writers_declared {
                    warnings.push(CheckWarning::UndeclaredParallelScope {
                        group: node.id.clone(),
                    });
                }
            }
            collect_parallel_warnings(children, warnings);
        }
    }
}

fn find_depends_on_cycle(nodes: &[Node]) -> Option<Vec<NodeId>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        /// Carries its own index in `stack`, so finding a gray node's
        /// position never needs a fallible search.
        Gray(usize),
        Black,
    }

    let adjacency: HashMap<NodeId, Vec<NodeId>> = nodes
        .iter()
        .map(|n| (n.id.clone(), n.depends_on.clone()))
        .collect();
    let mut color: HashMap<NodeId, Color> = adjacency
        .keys()
        .cloned()
        .map(|id| (id, Color::White))
        .collect();
    let mut stack: Vec<NodeId> = Vec::new();

    fn visit(
        id: &NodeId,
        adjacency: &HashMap<NodeId, Vec<NodeId>>,
        color: &mut HashMap<NodeId, Color>,
        stack: &mut Vec<NodeId>,
    ) -> Option<Vec<NodeId>> {
        color.insert(id.clone(), Color::Gray(stack.len()));
        stack.push(id.clone());

        if let Some(deps) = adjacency.get(id) {
            for dep in deps {
                if !adjacency.contains_key(dep) {
                    continue; // unknown dependency — reported separately
                }
                match color.get(dep).copied() {
                    Some(Color::Gray(pos)) => {
                        let mut cycle = stack[pos..].to_vec();
                        cycle.push(dep.clone());
                        return Some(cycle);
                    }
                    Some(Color::Black) => continue,
                    _ => {
                        if let Some(cycle) = visit(dep, adjacency, color, stack) {
                            return Some(cycle);
                        }
                    }
                }
            }
        }

        stack.pop();
        color.insert(id.clone(), Color::Black);
        None
    }

    for id in adjacency.keys() {
        if matches!(color.get(id), Some(Color::White)) {
            if let Some(cycle) = visit(id, &adjacency, &mut color, &mut stack) {
                return Some(cycle);
            }
        }
    }
    None
}

/// DI-13: config values the schema parses but nothing implements yet
/// must be refused, never accepted and ignored (A6). Today that is
/// `defaults.on_failure` beyond `pause` (the built behavior), and a
/// `defaults.runner` that `runners:` doesn't define.
fn check_config_defaults(config: &ConfigLayer, errors: &mut Vec<CheckError>) {
    let Some(defaults) = &config.defaults else {
        return;
    };
    if defaults.max_parallel_nodes == Some(0) {
        errors.push(CheckError::MaxParallelNodesZero);
    }
    if let Some(on_failure) = defaults.on_failure {
        if on_failure != yunta_core::DefaultOnFailure::Pause {
            errors.push(CheckError::DefaultOnFailureUnsupported { on_failure });
        }
    }
    if let Some(runner) = &defaults.runner {
        let defined = config
            .runners
            .as_ref()
            .and_then(|runners| runners.get(runner))
            .is_some_and(|candidates| !candidates.is_empty());
        if !defined {
            errors.push(CheckError::UnknownRunner {
                node: "defaults".into(),
                runner: runner.clone(),
            });
        }
    }
}

/// DI-13: `fresh_context: false` names a capability (session resume,
/// DI-23) that doesn't exist — error, never silent acceptance (A6).
fn check_fresh_context(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    for node in workflow.iter_nodes() {
        if node.fresh_context == Some(false) {
            errors.push(CheckError::FreshContextUnsupported {
                node: node.id.clone(),
            });
        }
        // DI-23: the explicit declaration is refused where no session
        // exists; the *config default* stays legal (it applies where a
        // session exists and means restart everywhere else).
        if node.on_interrupt == Some(yunta_core::OnInterrupt::ResumeSession)
            && !matches!(node.kind, NodeKind::Prompt { .. })
        {
            errors.push(CheckError::ResumeSessionOnSessionlessNode {
                node: node.id.clone(),
            });
        }
    }
}

/// DI-13/§2.1: `yunta_schema` is a space-separated list of comparators
/// over the schema major (`>=1 <2`, `=1`, `<3`…), all of which must
/// hold for [`yunta_core::YUNTA_SCHEMA`]. Deliberately a ~20-line
/// parser instead of a semver dependency: the schema version is one
/// integer, and the small static binary is a product feature.
fn check_yunta_schema(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let Some(range) = &workflow.yunta_schema else {
        return;
    };
    match yunta_schema_satisfied(range, yunta_core::YUNTA_SCHEMA) {
        Ok(true) => {}
        Ok(false) => errors.push(CheckError::YuntaSchemaMismatch {
            range: range.clone(),
            detail: "this binary's schema is outside the required range".to_string(),
            binary: yunta_core::YUNTA_SCHEMA,
        }),
        Err(detail) => errors.push(CheckError::YuntaSchemaMismatch {
            range: range.clone(),
            detail,
            binary: yunta_core::YUNTA_SCHEMA,
        }),
    }
}

/// `Ok(bool)` = every comparator evaluated against `binary`; `Err` = the
/// range doesn't parse. Empty ranges don't parse either — a declared
/// requirement that constrains nothing is a typo, not a wildcard.
fn yunta_schema_satisfied(range: &str, binary: u32) -> Result<bool, String> {
    let mut any = false;
    for comparator in range.split_whitespace() {
        let (op, number) = comparator
            .find(|c: char| c.is_ascii_digit())
            .map(|i| comparator.split_at(i))
            .ok_or_else(|| format!("comparator `{comparator}` has no version number"))?;
        let number: u32 = number
            .parse()
            .map_err(|_| format!("`{number}` is not a whole schema version"))?;
        let holds = match op {
            ">=" => binary >= number,
            "<=" => binary <= number,
            ">" => binary > number,
            "<" => binary < number,
            "=" | "==" | "" => binary == number,
            other => return Err(format!("unknown comparator `{other}`")),
        };
        any = true;
        if !holds {
            return Ok(false);
        }
    }
    if !any {
        return Err("the range is empty".to_string());
    }
    Ok(true)
}

/// DI-24: every `on_finish.distill` path must be some node's declared
/// artifact. Template-bearing names (`findings-{{runner.role}}.yaml`)
/// compare as written — the distill declaration must match the
/// production declaration, both pre-render.
fn check_distill_paths(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let mut produced: HashSet<&str> = HashSet::new();
    for node in workflow.iter_nodes() {
        if let Some(artifacts) = &node.artifacts {
            for spec in &artifacts.produces {
                produced.insert(match spec {
                    yunta_core::ArtifactSpec::Plain(name) => name,
                    yunta_core::ArtifactSpec::Typed { name, .. } => name,
                });
            }
        }
    }
    for step in &workflow.on_finish {
        let yunta_core::OnFinishStep::Distill { distill } = step else {
            continue;
        };
        for path in distill {
            if !produced.contains(path.as_str()) {
                errors.push(CheckError::DistillUnknownArtifact { path: path.clone() });
            }
        }
    }
}

/// T9.3's per-file workflow-node rules: no runner bindings (a workflow
/// node opens no session — the child's nodes bind their own), and an
/// `inherit` child of a `parallel` group must declare scope so §12's
/// disjointness demand is verifiable at all.
fn check_workflow_nodes(nodes: &[Node], group: Option<&Node>, errors: &mut Vec<CheckError>) {
    for node in nodes {
        if let NodeKind::Workflow { isolation, .. } = &node.kind {
            for (present, field) in [
                (node.runner.is_some(), "runner"),
                (!node.runners.is_empty(), "runners"),
                (node.agent.is_some(), "agent"),
            ] {
                if present {
                    errors.push(CheckError::WorkflowNodeRunnerBinding {
                        node: node.id.clone(),
                        field,
                    });
                }
            }
            if *isolation == yunta_core::WorkflowIsolation::Inherit && node.scope.is_empty() {
                if let Some(group) = group {
                    errors.push(CheckError::InheritChildWithoutScope {
                        group: group.id.clone(),
                        node: node.id.clone(),
                    });
                }
            }
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            check_workflow_nodes(children, Some(node), errors);
        }
    }
}

/// D108's declaration rules, on the original (pre-expansion) shape:
/// mounts live on top-level `kind: workflow` nodes, reference an
/// existing node other than themselves, and never appear inside a
/// `parallel` group (no order there — §12's "hermanos terminados"
/// cannot hold). `MountOnFanOut` lives in [`check_runner_fanout`],
/// next to the other fan-out target rules.
fn check_mounts(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let known: HashSet<&NodeId> = workflow.iter_nodes().map(|node| &node.id).collect();
    for node in &workflow.nodes {
        if let NodeKind::Workflow { mounts, .. } = &node.kind {
            for mount in mounts {
                let target = &mount.artifact.node;
                if target == &node.id {
                    errors.push(CheckError::MountOnSelf {
                        node: node.id.clone(),
                    });
                } else if !known.contains(target) {
                    errors.push(CheckError::MountUnknownNode {
                        node: node.id.clone(),
                        target: target.clone(),
                    });
                }
            }
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            for child in children {
                if let NodeKind::Workflow { mounts, .. } = &child.kind {
                    if !mounts.is_empty() {
                        errors.push(CheckError::MountInsideParallel {
                            group: node.id.clone(),
                            node: child.id.clone(),
                        });
                    }
                }
            }
        }
    }
}

/// T9.3/§12: walks the composition reference graph as the repo's
/// catalog stands **today** — every `use:` resolves, no cycles, and
/// nesting stays within `limits.max_workflow_depth`. A separate entry
/// point from [`check`], deliberately: `check` never reads files (its
/// own doc-comment rule), while this walk exists precisely to read the
/// catalog — the CLI calls both.
pub fn check_workflow_refs(
    workflow: &Workflow,
    config: &ConfigLayer,
    repo_root: &std::path::Path,
) -> Vec<CheckError> {
    let mut errors = Vec::new();
    let catalog = repo_root.join(".yunta/workflows");
    let max_depth = config.resolved_max_workflow_depth();
    let mut path: Vec<String> = Vec::new();
    walk_workflow_refs(workflow, &catalog, max_depth, &mut path, &mut errors);
    errors
}

/// Every `(node, use-name)` reference, `parallel` children included.
fn workflow_uses(workflow: &Workflow) -> Vec<(NodeId, String)> {
    workflow
        .iter_nodes()
        .filter_map(|node| match &node.kind {
            NodeKind::Workflow { r#use, .. } => Some((node.id.clone(), r#use.clone())),
            _ => None,
        })
        .collect()
}

fn walk_workflow_refs(
    workflow: &Workflow,
    catalog: &std::path::Path,
    max_depth: u32,
    path: &mut Vec<String>,
    errors: &mut Vec<CheckError>,
) {
    for (node, name) in workflow_uses(workflow) {
        if path.contains(&name) {
            let chain = path
                .iter()
                .cloned()
                .chain(std::iter::once(name.clone()))
                .collect::<Vec<_>>()
                .join(" -> ");
            errors.push(CheckError::WorkflowRefCycle { chain });
            continue;
        }
        let depth = path.len() as u32 + 1;
        if depth > max_depth {
            let chain = path
                .iter()
                .cloned()
                .chain(std::iter::once(name.clone()))
                .collect::<Vec<_>>()
                .join(" -> ");
            errors.push(CheckError::WorkflowRefTooDeep {
                chain,
                depth,
                max: max_depth,
            });
            continue;
        }
        let file = catalog.join(format!("{name}.yaml"));
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(_) => {
                errors.push(CheckError::WorkflowRefMissing {
                    node,
                    name,
                    path: file,
                });
                continue;
            }
        };
        let child: Workflow = match serde_yaml::from_str(&text) {
            Ok(child) => child,
            Err(e) => {
                errors.push(CheckError::WorkflowRefUnparseable {
                    path: file,
                    detail: e.to_string(),
                });
                continue;
            }
        };
        path.push(name);
        walk_workflow_refs(&child, catalog, max_depth, path, errors);
        path.pop();
    }
}

/// T9.4's declaration rules, checked before expansion.
fn check_runner_fanout(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let fanout_ids: HashSet<&NodeId> = workflow
        .nodes
        .iter()
        .filter(|node| !node.runners.is_empty())
        .map(|node| &node.id)
        .collect();
    for node in &workflow.nodes {
        if !node.runners.is_empty() && node.runner.is_some() {
            errors.push(CheckError::BothRunnerAndRunners {
                node: node.id.clone(),
            });
        }
        if let Some(on_failure) = &node.on_failure {
            if fanout_ids.contains(&on_failure.goto) {
                errors.push(CheckError::FanOutTarget {
                    node: node.id.clone(),
                    target: on_failure.goto.clone(),
                });
            }
        }
        if let NodeKind::Gate { on, .. } = &node.kind {
            for target in on.values() {
                if fanout_ids.contains(target) {
                    errors.push(CheckError::FanOutTarget {
                        node: node.id.clone(),
                        target: target.clone(),
                    });
                }
            }
        }
        for spec in &node.context {
            if let yunta_core::ContextSpec::Artifact { artifact } = spec {
                if let Some(referenced) = &artifact.node {
                    if fanout_ids.contains(referenced) {
                        errors.push(CheckError::FanOutTarget {
                            node: node.id.clone(),
                            target: referenced.clone(),
                        });
                    }
                }
            }
        }
        if let NodeKind::Workflow { mounts, .. } = &node.kind {
            for mount in mounts {
                if fanout_ids.contains(&mount.artifact.node) {
                    errors.push(CheckError::MountOnFanOut {
                        node: node.id.clone(),
                        target: mount.artifact.node.clone(),
                    });
                }
            }
        }
    }
    // An explicit `runners: []` parses identically to an absent field
    // (Vec + serde default), so it degrades to the ordinary
    // no-runner-declared path — reported there, never silently special-
    // cased here.
}
