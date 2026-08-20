//! `yunta check` (T1.3) — **M-0/M4 cut only**.
//!
//! Full T1.3 also validates mode coherence, template variables,
//! workflow-composition depth, permission ceilings and warns on pushes to
//! the base branch — all of it for schema surface (`modes:`, `context:`,
//! `permissions:`, composition) that this recorte doesn't have yet
//! (T1.1/T1.2). What's checked here is exactly what the recortado schema
//! can be wrong about:
//!
//! - node ids are unique, globally — including every `parallel` child,
//!   nested arbitrarily deep (T4.6);
//! - `depends_on` references exist and its graph is acyclic (I14: this
//!   check never looks at `on_failure.goto` — re-route edges are a
//!   separate set that never relaxes `depends_on` acyclicity);
//! - `on_failure.goto` targets exist;
//! - a node's `runner:` resolves to a role with at least one candidate in
//!   the merged config's `runners:`;
//! - a `parallel` group's children don't declare overlapping scope
//!   (D100/§5.8) — error, since it's verifiable in advance from the
//!   workflow alone (see `check_warnings` for the "can't verify" case).
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

    /// §9/T6.1: `context:` is resolved into a session's own prompt —
    /// only `kind: prompt` opens one in this recorte (a `bash`/`check`/
    /// `executor`/`loop` node has nowhere to put it yet; see
    /// `docs/m0-status.md`'s T6.1 entry). Declaring it elsewhere is
    /// caught here rather than silently ignored at runtime (A6).
    #[error("node `{node}`: `context:` is only supported on `kind: prompt` nodes in this recorte")]
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
    crate::manifest::expand_implicit_dependencies(&mut workflow);
    let workflow = &workflow;

    let mut errors = Vec::new();

    // Global, not per-group: replay derives node state from one flat
    // NodeId -> NodeState map (I2), so a `parallel` child's id colliding
    // with anything else — a sibling, a top-level node, another group's
    // child — would corrupt derivation, not just read oddly.
    let mut known_ids: HashSet<NodeId> = HashSet::new();
    collect_ids(&workflow.nodes, &mut known_ids, &mut errors);

    check_parallel_scopes(&workflow.nodes, &mut errors);
    check_fanout_scopes(workflow, config, &mut errors);
    check_fresh_context(&workflow.nodes, &mut errors);
    check_yunta_schema(workflow, &mut errors);

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

        if !node.context.is_empty() && !matches!(node.kind, NodeKind::Prompt { .. }) {
            errors.push(CheckError::ContextOnUnsupportedNode {
                node: node.id.clone(),
            });
        }

        check_gate(node, &known_ids, config, &mut errors);
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
    warnings
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

fn collect_ids(nodes: &[Node], known_ids: &mut HashSet<NodeId>, errors: &mut Vec<CheckError>) {
    for node in nodes {
        if !known_ids.insert(node.id.clone()) {
            errors.push(CheckError::DuplicateNodeId {
                id: node.id.clone(),
            });
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            collect_ids(children, known_ids, errors);
        }
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

/// DI-13: `fresh_context: false` names a capability (session resume,
/// DI-23) that doesn't exist — error, never silent acceptance (A6).
fn check_fresh_context(nodes: &[Node], errors: &mut Vec<CheckError>) {
    for node in nodes {
        if node.fresh_context == Some(false) {
            errors.push(CheckError::FreshContextUnsupported {
                node: node.id.clone(),
            });
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            check_fresh_context(children, errors);
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
