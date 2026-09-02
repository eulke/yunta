//! See [`super`]. One family of workflow-check rules.

use super::*;
use thiserror::Error;

/// A `yunta_schema:` range the parser cannot read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SchemaRangeError {
    #[error("the range is empty")]
    Empty,
    #[error("comparator `{comparator}` has no version number")]
    NoVersion { comparator: String },
    #[error("`{text}` is not a whole schema version")]
    NotAVersion { text: String },
    #[error("unknown comparator `{op}`")]
    UnknownOperator { op: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckError {
    #[error("duplicate node id `{id}`")]
    DuplicateNodeId { id: NodeId },

    /// A node's field names a target node the workflow doesn't define — the
    /// one broken-reference error, whatever field carries the reference
    /// (`depends_on`, `on_failure.goto`, a gate option's `on.<option>`, a
    /// `mounts` entry). Catching it here means the run never starts having
    /// silently dropped work its author named.
    #[error("node `{node}`: `{field}` references unknown node `{target}`")]
    BrokenReference {
        node: NodeId,
        field: String,
        target: NodeId,
    },

    #[error("cycle in depends_on: {path}")]
    DependsOnCycle { path: String },

    #[error("node `{node}` references runner `{runner}`, which `runners:` does not define")]
    UnknownRunner { node: NodeId, runner: RunnerName },

    #[error(
        "node `{node}` references runner `{runner}`, which `runners:` defines with zero candidates"
    )]
    RunnerHasNoCandidates { node: NodeId, runner: RunnerName },

    /// `parallel`'s children share one worktree — a scope
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

    /// `defaults.on_failure` values beyond `pause` (today's
    /// behavior) have no implementation — refused, never silently read
    /// as `pause`.
    #[error(
        "`defaults.on_failure: {on_failure:?}` is not built yet — only `pause` is; remove \
         the field or declare `pause`"
    )]
    DefaultOnFailureUnsupported {
        on_failure: yunta_core::DefaultOnFailure,
    },

    /// `runner:` and `runners:` on one node is a contradiction,
    /// not a merge.
    #[error("node `{node}` declares both `runner:` and `runners:` — use exactly one")]
    BothRunnerAndRunners { node: NodeId },

    /// Once `review` is many nodes there is no unambiguous
    /// "return control to review" — a re-route, gate `on:` or context
    /// artifact reference must name a specific node.
    #[error(
        "node `{node}` targets `{target}`, which is a `runners:` fan-out — target one of its \
         expanded nodes (`{target}@<role>`) or a non-fan-out node"
    )]
    FanOutTarget { node: NodeId, target: NodeId },

    /// `on_finish.distill` names a path no node declares
    /// producing — statically wrong (the runtime "declared but not
    /// produced this run" case degrades to a finding instead).
    #[error(
        "`on_finish.distill` names `{path}` but no node's `artifacts.produces` declares it — \
         declare the artifact on the node that writes it, or drop it from `distill`"
    )]
    DistillUnknownArtifact { path: String },

    /// An artifact is written under `run.dir/artifacts/`; a name that
    /// is absolute or climbs with `..` would land somewhere else.
    #[error(
        "node `{node}` produces `{name}` — an artifact name is a relative path with no `..` \
         component, so it stays under the run's `artifacts/`"
    )]
    ArtifactNameEscapes { node: NodeId, name: String },

    /// The workflow demands a schema this binary doesn't speak.
    #[error(
        "`yunta_schema: \"{range}\"` — this binary's schema is outside the required range (this \
         binary speaks schema {binary})"
    )]
    YuntaSchemaOutside { range: String, binary: u32 },

    /// The workflow's `yunta_schema:` range cannot be read.
    #[error("`yunta_schema: \"{range}\"` — {source} (this binary speaks schema {binary})")]
    YuntaSchemaUnreadable {
        range: String,
        binary: u32,
        #[source]
        source: SchemaRangeError,
    },

    /// A pack-origin workflow's `pack.yaml` exists but cannot be read —
    /// its `declares.permissions` ceiling is the pack's only governance
    /// rule, so a manifest the check cannot read is a check error naming
    /// the file, never a silently skipped ceiling.
    #[error("pack manifest `{path}` cannot be read: {detail}")]
    PackManifestUnreadable {
        path: std::path::PathBuf,
        detail: String,
    },

    /// A pack-origin workflow's `pack.yaml` does not parse — same
    /// reasoning as [`CheckError::PackManifestUnreadable`].
    #[error("pack manifest `{path}` is malformed: {detail}")]
    PackManifestMalformed {
        path: std::path::PathBuf,
        detail: String,
    },

    /// The same worktree-collision rule extended to the DAG's
    /// *implicit* fan-out — two top-level nodes with no dependency path
    /// between them can be `ready` together, and with
    /// `max_parallel_nodes > 1` they share one worktree at once,
    /// exactly the physical risk `parallel` already errors on. Static
    /// approximation by design: "no relative order declared" is the
    /// rule, never a simulation of what the scheduler would actually
    /// interleave.
    #[error(
        "nodes `{a}` and `{b}` have no dependency path between them and declare overlapping \
         scope (`{glob_a}` / `{glob_b}`) — with `max_parallel_nodes` > 1 they can write the \
         same paths at once; chain them with `depends_on` or make their scopes disjoint"
    )]
    OverlappingFanOutScope {
        a: NodeId,
        b: NodeId,
        glob_a: String,
        glob_b: String,
    },

    /// The first enforcement moment: the command as written in
    /// the YAML already violates the merged `permissions` model. The scan
    /// matches the *literal* text — a command assembled by template gets
    /// caught by the second moment, at runtime, right before execution.
    #[error("node `{node}`: {rule}")]
    CommandDenied { node: NodeId, rule: String },

    /// `context:` is resolved into a session's own
    /// prompt — `kind: prompt` (the node's one session) and `kind:
    /// loop` (once per task brief) are the kinds that open one; a
    /// `bash`/`check`/`executor`/`gate` node has no session to consume
    /// it. Declaring it there is caught here rather than silently
    /// ignored at runtime.
    #[error(
        "node `{node}`: `context:` is only supported on `kind: prompt` and `kind: loop` nodes \
         — nothing else opens a session that could consume it"
    )]
    ContextOnUnsupportedNode { node: NodeId },

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

    /// `check` verifies that every `{{{{inputs.x}}}}` refers to a
    /// declared input — scanned wherever a template can appear inline
    /// in the workflow (prompt text, `bash`/hook commands, `context:`
    /// patterns and command/query text). A `prompt: {file: ...}` body
    /// isn't scanned: `check` never reads files (see this module's own
    /// doc comment), so an undeclared reference there still only
    /// surfaces at run time.
    #[error("node `{node}` references `{{{{inputs.{name}}}}}`, which `inputs:` does not declare")]
    UndeclaredInput { node: NodeId, name: String },

    /// A `kind: gate` with `external:` has nowhere to
    /// actually publish without a forge — refused here rather than at
    /// runtime, the same "check catches what a run would only
    /// discover after spending something" reasoning `UnknownRunner`
    /// already applies. This checks only that a forge is *configured*
    /// — a specific machine lacking the named credential env var at
    /// *runtime* is a different, degrade-not-refuse case (it degrades
    /// to console instead of failing).
    #[error("node `{node}`: `kind: gate` with `external: {{kind: pull_request}}` needs `forge.github` configured")]
    ExternalGateWithoutForge { node: NodeId },

    /// A gate's resolution is a forge round-trip, one at a time — never
    /// scoped to a `parallel` group's shared worktree/join semantics
    /// (neither concept is defined for a gate).
    #[error("node `{node}`: `kind: gate` can't be a `parallel` child (group `{group}`)")]
    GateInsideParallel { node: NodeId, group: NodeId },

    /// `on:` may only map options the gate itself declares —
    /// mapping an undeclared one is a choice no human can ever make.
    #[error("gate `{node}`: `on.{option}` maps an option `options:` does not declare")]
    GateOnUndeclaredOption { node: NodeId, option: String },

    /// The same broken-reference class as `BrokenReference` —
    /// catching it here means the run never starts with a mode that
    /// silently omits work its own author meant to include.
    #[error("mode `{mode}` includes unknown node `{node}`")]
    ModeReferencesUnknownNode { mode: ModeName, node: NodeId },

    /// A mode trims deliberation, never verification — checked
    /// independent of the mode's name or count.
    #[error("node `{node}` is `invariant: true` but mode `{mode}` doesn't include it")]
    InvariantNodeExcludedFromMode { node: NodeId, mode: ModeName },

    /// A mode's own coherence rule, made an error rather than a warning
    /// for the same reason: it's the same broken-reference class
    /// `BrokenReference` catches, just scoped to one mode's variant of
    /// the graph instead of the whole file. The message names both ways
    /// out.
    #[error(
        "node `{node}` is in mode `{mode}`, but its on_failure.goto target `{goto}` isn't — \
         include `{goto}` in `{mode}`, or drop the re-route there"
    )]
    RerouteTargetExcludedFromMode {
        mode: ModeName,
        node: NodeId,
        goto: NodeId,
    },

    /// A `kind: workflow` node never opens a session of its own —
    /// the child's nodes bind their own runners — so a runner binding
    /// here would be accepted and ignored, which the engine never
    /// allows silently.
    #[error(
        "node `{node}`: `{field}` has no meaning on `kind: workflow` — the child workflow's \
         own nodes bind their runners"
    )]
    WorkflowNodeRunnerBinding { node: NodeId, field: &'static str },

    /// A parallel child with `isolation: inherit` shares the parent's
    /// one tree with every concurrent sibling, so an undeclared scope
    /// makes disjointness unverifiable: refused, same rank as
    /// `OverlappingParallelScope` (which catches the declared-overlap
    /// half of the same rule).
    #[error(
        "parallel group `{group}`: child `{node}` is `kind: workflow` with `isolation: \
         inherit` and no `scope` — inherit children share the parent's tree, so each must \
         declare a disjoint scope"
    )]
    InheritChildWithoutScope { group: NodeId, node: NodeId },

    /// Mounting one's own artifact is a read of an outcome that
    /// cannot exist yet — the implied `depends_on` would be a self-cycle.
    #[error(
        "node `{node}`: `mounts` references the node itself — a mount reads a *finished* \
         node's artifact, which this node cannot be for its own birth"
    )]
    MountOnSelf { node: NodeId },

    /// Same reasoning as `FanOutTarget` — once `runners:` expands
    /// a node into `<id>@<role>` siblings there is no "the" node to
    /// mount from.
    #[error(
        "node `{node}`: `mounts` references `{target}`, which `runners:` fans out into one \
         node per role — mount a specific `{target}@<role>` sibling instead"
    )]
    MountOnFanOut { node: NodeId, target: NodeId },

    /// Parallel children run concurrently — no DAG order exists
    /// inside the group, so "the source node already finished" cannot
    /// hold there and the implied `depends_on` would mean nothing.
    #[error(
        "parallel group `{group}`: child `{node}` declares `mounts` — parallel children have \
         no order to guarantee a finished source; mount on a top-level workflow node instead"
    )]
    MountInsideParallel { group: NodeId, node: NodeId },

    /// Only `kind: prompt` opens a node-scoped session —
    /// declaring `resume_session` anywhere else names a conversation
    /// that doesn't exist (a loop's per-task sessions re-run from the
    /// ledger; bash/check/executor/gate/workflow open none).
    #[error(
        "node `{node}`: `on_interrupt: resume_session` is only supported on `kind: prompt`          nodes — nothing else has a node-scoped session to resume; declare `restart_node`          (the default) instead"
    )]
    ResumeSessionOnSessionlessNode { node: NodeId },

    /// The node asks for a scope-expansion mode more
    /// permissive than the merged `permissions.scope_expansion.max_mode`
    /// ceiling allows — same only-narrowing model as every other
    /// `permissions` group; which *layer* set the binding ceiling
    /// is `permission_layer_conflicts`' territory at config load.
    #[error(
        "node `{node}`: `scope_expansion.mode: {mode}` exceeds the merged permissions ceiling \
         `scope_expansion.max_mode: {ceiling}` — harden the node's mode, or raise \
         the ceiling in the layer that set it"
    )]
    ScopeExpansionModeOverCeiling {
        node: NodeId,
        mode: &'static str,
        ceiling: &'static str,
    },

    /// A 0 would starve every ready node forever — a config
    /// mistake surfaced here as a refusal (the scheduler's clamp to 1
    /// stays as defense in depth).
    #[error(
        "`defaults.max_parallel_nodes: 0` would starve every node forever — declare 1 or more, \
         or drop the field (default: 1)"
    )]
    MaxParallelNodesZero,

    /// A composition reference that can't resolve today — the
    /// same broken-reference class as `BrokenReference`, across
    /// files. Advisory about the *current* catalog by design: the child
    /// freezes its own file at birth, so a run only ever meets the file
    /// as it is then.
    #[error("node `{node}`: `use: {name}` cannot be resolved — {detail}")]
    WorkflowRefMissing {
        node: NodeId,
        name: String,
        detail: String,
    },

    /// Two packs installed under the same publisher each declare a
    /// workflow with the same file basename — the flat
    /// `publisher/workflow` namespace can't tell them apart.
    #[error("node `{node}`: `use: {name}` is ambiguous — {detail}")]
    AmbiguousWorkflowRef {
        node: NodeId,
        name: String,
        detail: String,
    },

    /// Cross-pack references aren't supported — a workflow
    /// that lives inside a pack may only `use:` other workflows from
    /// that same pack, never the repo's own catalog or a different
    /// pack (no transitive pack dependencies).
    #[error(
        "node `{node}`: `use: {name}` reaches outside pack `{from_pack}` — composition across \
         packs isn't supported; copy what you need into your own pack instead"
    )]
    CrossPackWorkflowRef {
        node: NodeId,
        name: String,
        from_pack: String,
    },

    #[error("workflow `{path}` (referenced through composition) does not parse: {detail}")]
    WorkflowRefUnparseable {
        path: std::path::PathBuf,
        detail: String,
    },

    /// The graph of references between workflows must be acyclic.
    #[error("workflow composition cycle: {chain}")]
    WorkflowRefCycle { chain: String },

    /// The configurable maximum nesting depth, checked statically over
    /// the reference graph (the runtime guard at child birth enforces
    /// the same limit over what actually loads).
    #[error(
        "workflow composition {chain} nests {depth} level(s) deep but \
         `limits.max_workflow_depth` is {max} — flatten the composition or raise the limit"
    )]
    WorkflowRefTooDeep { chain: String, depth: u32, max: u32 },

    /// A pack's `declares` field is a ceiling, not a description — a
    /// pack's own `prompt`/`loop` node can never request a session
    /// profile above what its manifest promises, even when the node's
    /// own YAML asks for more (or asks for nothing and falls back to
    /// the engine's own `edit` default).
    #[error(
        "node `{node}` requests permissions `{effective}` but pack `{pack}` declares a ceiling \
         of `{declared}` — lower the node's permissions or raise the pack's declared ceiling"
    )]
    PackPermissionsCeilingExceeded {
        node: NodeId,
        pack: String,
        declared: &'static str,
        effective: &'static str,
    },
}

/// A non-blocking finding — the run can still start (`check`
/// warns, it doesn't refuse, when a collision can't be verified for lack
/// of declared scope). Kept separate from `CheckError` rather than adding
/// a severity field to it: every existing caller of `check()` keeps
/// treating its `Vec<CheckError>` as "must be empty to proceed" without
/// learning to filter by severity.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckWarning {
    #[error(
        "parallel group `{group}`: two or more children can write and don't declare scope as \
         disjoint — the engine can't verify they won't collide; declare `scope` on each \
         to make the check real"
    )]
    UndeclaredParallelScope { group: NodeId },

    /// The fan-out analogue of `UndeclaredParallelScope` — one
    /// warning per connected component of mutually-independent,
    /// write-capable, scope-less top-level nodes (per pair would drown
    /// the signal in noise).
    #[error(
        "nodes {nodes} have no dependency paths between them and can all write without \
         declared scope — with `max_parallel_nodes` > 1 the engine can't verify they won't \
         collide; declare `scope` on each or chain them with `depends_on`"
    )]
    UndeclaredFanOutScope { nodes: String },

    /// A literal `git push` aimed at the base branch with no
    /// gate anywhere before it in the DAG — warning, not error: a team
    /// may genuinely want it, but nobody should discover an ungated
    /// push to `main` from the push itself.
    #[error(
        "node `{node}` pushes to the base branch (`{branch}`) with no gate anywhere before it \
         in the DAG — put a gate ahead of the push, or push to `{{{{run.branch}}}}`"
    )]
    PushToBaseWithoutGate { node: NodeId, branch: String },
}
