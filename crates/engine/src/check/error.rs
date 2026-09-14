//! See [`super`]. One family of workflow-check rules.

use super::*;
use thiserror::Error;
use yunta_core::OptionId;

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

    /// A node asks its adapter for something the adapter does not
    /// declare and the engine never emulates. Refused before a run is
    /// born: a session that ignored the profile or the agent it was
    /// given would run under something nobody asked for.
    #[error(
        "node `{node}` declares `{field}`, which needs `{capability}`, and no adapter its \
         runner resolves to declares it ({adapters}) — pick a runner on an adapter that has it"
    )]
    CapabilityUnsupported {
        node: NodeId,
        field: String,
        capability: yunta_core::Capability,
        adapters: String,
    },
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
        "`on_finish.distill` names {artifact} but that node's `artifacts.produces` does not \
         declare it — declare the artifact on the node that produces it, or drop it from \
         `distill`"
    )]
    DistillUnknownArtifact {
        artifact: yunta_core::DistillArtifact,
    },

    /// A node produces at most one document of each kind: the identity
    /// is `(node, kind)`, so a second declaration names the first.
    #[error(
        "node `{node}` declares `{kind}` twice — a node produces at most one {label}, and it \
         is identified by its kind, so there is no second one to declare",
        label = .kind.label()
    )]
    DuplicateArtifactKind {
        node: NodeId,
        kind: yunta_core::ArtifactKind,
    },

    /// An input document and a node producing the same kind are two
    /// producers of one identity with nothing to order them.
    #[error(
        "input `{input}` brings in the {label} and node `{node}` produces one too — a run \
         resolving `{kind}` would get whichever landed last, which nothing in the workflow \
         decides; drop the input, or `{kind}` from node `{node}`'s `artifacts.produces`",
        label = .kind.label()
    )]
    InputDocumentAlsoProduced {
        input: String,
        node: NodeId,
        kind: yunta_core::ArtifactKind,
    },

    /// The three kind names are how a document the engine reads is
    /// referred to, so none of them is available as a file name.
    #[error(
        "{site} names the artifact `{name}`, and `{name}` is a document Yunta reads — refer to \
         it with `kind: {name}` instead; the names {kinds} are not available as file names",
        kinds = yunta_core::ArtifactKind::listed()
    )]
    ReservedArtifactName { site: String, name: String },

    /// An artifact is written under `run.dir/artifacts/`; a name that
    /// is absolute or climbs with `..` would land somewhere else.
    /// The name a node declares is not one an artifact can take. The
    /// clause comes from the name's own parser, so `check` and the run
    /// refuse the same names for the same stated reason.
    #[error("node `{node}` produces a name that {said}")]
    ArtifactNameRefused { node: NodeId, said: String },

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

    /// A node that asks ends when it asks: its answers are the next
    /// node's context, so nothing it declares beside `questions` could
    /// be written after them.
    #[error(
        "node `{node}` produces `questions` alongside {} — a node that asks ends when it asks, \
         and its answers reach the next node as context; keep `{node}` producing `questions` \
         alone and move {} to a node that follows it with `context: [{{ artifact: {{ node: \
         {node}, {} }} }}]`",
        ArtifactSpec::listed(.others), ArtifactSpec::listed(.others),
        yunta_core::ReservedIdentity::Answers.reference()
    )]
    QuestionsAlongsideOtherArtifacts {
        node: NodeId,
        others: Vec<ArtifactSpec>,
    },

    /// Only a `prompt` node holds the session that hands questions over
    /// and the close that waits on them.
    #[error(
        "node `{node}` is `kind: {kind}` and produces `questions` — only a `prompt` node asks; \
         put the questions in a `prompt` node and read its answers from here"
    )]
    QuestionsOnKind { node: NodeId, kind: &'static str },

    /// The scheduler puts questions to a person one top-level node at a
    /// time, so a group's child is never asked — its wait would never
    /// end and the node after the group would mount answers that never
    /// arrive.
    #[error(
        "node `{node}` produces `questions` inside parallel group `{group}` — a person answers \
         one node at a time; ask before or after the group"
    )]
    QuestionsInsideParallel { node: NodeId, group: NodeId },

    /// `on:` may only map options the gate itself declares —
    /// mapping an undeclared one is a choice no human can ever make.
    #[error("gate `{node}`: `on.{option}` maps an option `options:` does not declare")]
    GateOnUndeclaredOption { node: NodeId, option: OptionId },

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
    /// tasks document; bash/check/executor/gate/workflow open none).
    #[error(
        "node `{node}`: `on_interrupt: resume_session` is only supported on `kind: prompt` \
         nodes — nothing else has a node-scoped session to resume; declare `restart_node` \
         (the default) instead"
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
