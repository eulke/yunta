//! See [`super`]. One family of workflow-check rules.

use super::*;
use thiserror::Error;
use yunta_core::events::NameProblem;
use yunta_core::{InputName, SchemaRange, ScopeGlob};

/// What refuses a workflow: something its frozen inputs make certain to
/// stop the run, found ahead of any spend. Its `Display` is the
/// variant's code; the words a person reads are written where they are
/// shown, from the data each variant holds.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckError {
    #[error("depends_on_cycle")]
    DependsOnCycle { cycle: Vec<NodeId> },

    /// A node asks its adapter for something the adapter does not
    /// declare and the engine never emulates. Refused before a run is
    /// born: a session that ignored the profile or the agent it was
    /// given would run under something nobody asked for.
    #[error("capability_unsupported")]
    CapabilityUnsupported {
        node: NodeId,
        field: String,
        capability: yunta_core::Capability,
        adapters: String,
    },
    #[error("unknown_runner")]
    UnknownRunner {
        node: NodeId,
        runner: RunnerName,
        /// The runner `runners:` does define that is one slip away.
        near: Option<String>,
    },

    #[error("runner_has_no_candidates")]
    RunnerHasNoCandidates { node: NodeId, runner: RunnerName },

    /// A config key the node's kind cannot run without, left unset. A
    /// run freezes its config when it is created, so the node would stop
    /// the run every time it is reached.
    #[error("unset")]
    Unset {
        node: NodeId,
        key: yunta_core::ConfigKey,
        /// The name the config does declare that is one slip away from
        /// the one the node asks for.
        near: Option<String>,
    },

    /// A literal `files:` path the tree a run starts from does not hold,
    /// read by a node nothing that runs before can write it for.
    #[error("context_file_unreachable")]
    ContextFileUnreachable {
        node: NodeId,
        path: String,
        base: Option<String>,
        missing: MissingContextFile,
    },

    /// A read nothing in the run can answer, so the node that makes it
    /// stops every time it runs.
    #[error(transparent)]
    Unanswerable(#[from] Unanswerable),

    /// `runner:` and `runners:` on one node is a contradiction,
    /// not a merge.
    #[error("both_runner_and_runners")]
    BothRunnerAndRunners { node: NodeId },

    /// Once `review` is many nodes there is no unambiguous
    /// "return control to review" — a re-route, gate `on:` or context
    /// artifact reference must name a specific node.
    #[error("fan_out_target")]
    FanOutTarget { node: NodeId, target: NodeId },

    /// `on_finish.distill` names a path no node declares
    /// producing — statically wrong (the runtime "declared but not
    /// produced this run" case degrades to a finding instead).
    #[error("distill_unknown_artifact")]
    DistillUnknownArtifact {
        artifact: yunta_core::DistillArtifact,
    },

    /// A node produces at most one document of each kind: the identity
    /// is `(node, kind)`, so a second declaration names the first.
    #[error("duplicate_artifact_kind")]
    DuplicateArtifactKind {
        node: NodeId,
        kind: yunta_core::ArtifactKind,
    },

    /// An input document and a node producing the same kind are two
    /// producers of one identity with nothing to order them.
    #[error("input_document_also_produced")]
    InputDocumentAlsoProduced {
        input: InputName,
        node: NodeId,
        kind: yunta_core::ArtifactKind,
    },

    /// The three kind names are how a document the engine reads is
    /// referred to, so none of them is available as a file name.
    #[error("reserved_artifact_name")]
    ReservedArtifactName { place: Place, name: String },

    /// An artifact is written under `run.dir/artifacts/`; a name that
    /// is absolute or climbs with `..` would land somewhere else.
    /// The name a node declares is not one an artifact can take. The
    /// reason comes from the name's own parser, so `check` and the run
    /// refuse the same names for the same stated reason.
    #[error("artifact_name_refused")]
    ArtifactNameRefused {
        node: NodeId,
        /// The name as the node writes it, which the reason quotes.
        name: String,
        problem: NameProblem,
    },

    /// The workflow demands a schema this binary doesn't speak.
    #[error("yunta_schema_outside")]
    YuntaSchemaOutside { range: SchemaRange, binary: u32 },

    /// A pack-origin workflow's `pack.yaml` exists but cannot be read —
    /// its `declares.permissions` ceiling is the pack's only governance
    /// rule, so a manifest the check cannot read is a check error naming
    /// the file, never a silently skipped ceiling.
    #[error("pack_manifest_unreadable")]
    PackManifestUnreadable {
        path: std::path::PathBuf,
        detail: String,
    },

    /// A pack-origin workflow's `pack.yaml` does not parse — same
    /// reasoning as [`CheckError::PackManifestUnreadable`].
    #[error("pack_manifest_malformed")]
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
    #[error("overlapping_fan_out_scope")]
    OverlappingFanOutScope {
        a: NodeId,
        b: NodeId,
        glob_a: ScopeGlob,
        glob_b: ScopeGlob,
    },

    /// The first enforcement moment: the command as written in
    /// the YAML already violates the merged `permissions` model. The scan
    /// matches the *literal* text — a command assembled by template gets
    /// caught by the second moment, at runtime, right before execution.
    #[error("command_denied")]
    CommandDenied { node: NodeId, rule: String },

    /// `context:` is resolved into a session's own
    /// prompt — `kind: prompt` (the node's one session) and `kind:
    /// loop` (once per task brief) are the kinds that open one; a
    /// `bash`/`check`/`executor`/`gate` node has no session to consume
    /// it. Declaring it there is caught here rather than silently
    /// ignored at runtime.
    #[error("context_on_unsupported_node")]
    ContextOnUnsupportedNode { node: NodeId },

    #[error("input_empty_enum_values")]
    InputEmptyEnumValues { name: InputName },

    #[error("input_min_exceeds_max")]
    InputMinExceedsMax {
        name: InputName,
        min: String,
        max: String,
    },

    #[error("input_invalid_pattern")]
    InputInvalidPattern {
        name: InputName,
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
    /// The reference as written: what an author reads back, whether or
    /// not it could have been an input name at all.
    #[error("undeclared_input")]
    UndeclaredInput { node: NodeId, name: String },

    /// A `kind: gate` with `external:` has nowhere to
    /// actually publish without a forge — refused here rather than at
    /// runtime, the same "check catches what a run would only
    /// discover after spending something" reasoning `UnknownRunner`
    /// already applies. This checks only that a forge is *configured*
    /// — a specific machine lacking the named credential env var at
    /// *runtime* is a different, degrade-not-refuse case (it degrades
    /// to console instead of failing).
    #[error("external_gate_without_forge")]
    ExternalGateWithoutForge { node: NodeId },

    /// An external gate is decided on the forge, which shows its
    /// `artifacts:`; nothing here would ever put a `shows:` in front of
    /// anyone.
    #[error("shows_on_external_gate")]
    ShowsOnExternalGate { node: NodeId },

    /// A gate's resolution is a forge round-trip, one at a time — never
    /// scoped to a `parallel` group's shared worktree/join semantics
    /// (neither concept is defined for a gate).
    #[error("gate_inside_parallel")]
    GateInsideParallel { node: NodeId, group: NodeId },

    /// A group inside a group. Every surface draws a group's children
    /// one step under it, and the scheduler pairs each node with the
    /// group that holds it — both are exact at one level and false at
    /// two. Nesting arrives the day somebody asks for it, with the
    /// design it needs.
    #[error("parallel_inside_parallel")]
    ParallelInsideParallel { node: NodeId, group: NodeId },

    /// A node that asks ends when it asks: its answers are the next
    /// node's context, so nothing it declares beside `questions` could
    /// be written after them.
    #[error("questions_alongside_other_artifacts")]
    QuestionsAlongsideOtherArtifacts {
        node: NodeId,
        others: Vec<ArtifactSpec>,
    },

    /// The engine writes the answers to a `questions` document, so a
    /// node that declared them would owe a document nobody can hand it.
    #[error("answers_declared_as_produced")]
    AnswersDeclaredAsProduced { node: NodeId },

    /// Answers exist because a node asked, so a node that asks nothing
    /// has none to read.
    #[error("answers_from_node_that_never_asks")]
    AnswersFromNodeThatNeverAsks { node: NodeId, place: Place },

    /// Only a `prompt` node holds the session that hands questions over
    /// and the close that waits on them.
    #[error("questions_on_kind")]
    QuestionsOnKind { node: NodeId, kind: &'static str },

    /// The scheduler puts questions to a person one top-level node at a
    /// time, so a group's child is never asked — its wait would never
    /// end and the node after the group would mount answers that never
    /// arrive.
    #[error("questions_inside_parallel")]
    QuestionsInsideParallel { node: NodeId, group: NodeId },

    /// An `on:` route no answer ever follows: a promise the gate cannot keep.
    #[error("gate_route_never_taken")]
    GateRouteNeverTaken { node: NodeId, route: UntakenRoute },

    /// A `kind: workflow` node never opens a session of its own —
    /// the child's nodes bind their own runners — so a runner binding
    /// here would be accepted and ignored, which the engine never
    /// allows silently.
    #[error("workflow_node_runner_binding")]
    WorkflowNodeRunnerBinding { node: NodeId, field: &'static str },

    /// A parallel child with `isolation: none` shares the parent's
    /// one tree with every concurrent sibling, so an undeclared scope
    /// makes disjointness unverifiable: refused, same rank as
    /// `OverlappingParallelScope` (which catches the declared-overlap
    /// half of the same rule).
    #[error("inherit_child_without_scope")]
    InheritChildWithoutScope { group: NodeId, node: NodeId },

    /// Mounting one's own artifact is a read of an outcome that
    /// cannot exist yet — the implied `depends_on` would be a self-cycle.
    #[error("mount_on_self")]
    MountOnSelf { node: NodeId },

    /// Same reasoning as `FanOutTarget` — once `runners:` expands
    /// a node into `<id>@<role>` siblings there is no "the" node to
    /// mount from.
    #[error("mount_on_fan_out")]
    MountOnFanOut { node: NodeId, target: NodeId },

    /// Parallel children run concurrently — no DAG order exists
    /// inside the group, so "the source node already finished" cannot
    /// hold there and the implied `depends_on` would mean nothing.
    #[error("mount_inside_parallel")]
    MountInsideParallel { group: NodeId, node: NodeId },

    /// Only `kind: prompt` opens a node-scoped session —
    /// declaring `resume_session` anywhere else names a conversation
    /// that doesn't exist (a loop's per-task sessions re-run from the
    /// tasks document; bash/check/executor/gate/workflow open none).
    #[error("resume_session_on_sessionless_node")]
    ResumeSessionOnSessionlessNode { node: NodeId },

    /// `ask` says which rounds of a node's questions stop the run; on a
    /// node that hands over no `questions` it says nothing.
    #[error("ask_on_a_node_that_asks_nothing")]
    AskOnANodeThatAsksNothing { node: NodeId },

    /// `network: true` on a session that runs read-only, which gets none.
    #[error("network_open_under_read_only")]
    NetworkOpenUnderReadOnly { node: NodeId },

    /// A closed network on a session that runs unconfined, which nothing keeps closed.
    #[error("closed_network_unconfined")]
    ClosedNetworkUnconfined { node: NodeId },

    /// The node asks for a scope-expansion mode more
    /// permissive than the merged `permissions.scope_expansion.max_mode`
    /// ceiling allows — same only-narrowing model as every other
    /// `permissions` group; which *layer* set the binding ceiling
    /// is `permission_layer_conflicts`' territory at config load.
    #[error("scope_expansion_mode_over_ceiling")]
    ScopeExpansionModeOverCeiling {
        node: NodeId,
        mode: &'static str,
        ceiling: &'static str,
    },

    /// `scope_expansion.within` is retired: a loop that still declares it
    /// is refused, because the key would otherwise be read as a ceiling
    /// it no longer is.
    #[error("scope_expansion_within_retired")]
    ScopeExpansionWithinRetired { node: NodeId },

    /// A 0 would starve every ready node forever — a config
    /// mistake surfaced here as a refusal (the scheduler's clamp to 1
    /// stays as defense in depth).
    #[error("max_parallel_nodes_zero")]
    MaxParallelNodesZero,

    /// A `use:` reference to another workflow, refused.
    #[error(transparent)]
    Composition(#[from] super::refs::Composition),

    /// A comparison against a baseline nothing measures: the lineage
    /// measures once, before its first node, and only the suite the
    /// config names.
    #[error("baseline_without_suite")]
    BaselineWithoutSuite { sites: String },

    /// A config key that names a project command the project does not
    /// declare — a `format:` rule, a command `baseline.held` holds tasks
    /// to — would do nothing, with nobody told why.
    #[error("command_unset_in")]
    CommandUnsetIn {
        field: &'static str,
        command: yunta_core::CommandName,
        near: Option<String>,
    },

    /// A pack says what it needs from the project and the machine under
    /// `requires:`; a run of its workflows without it stops where the need
    /// is.
    #[error("pack_requirement_unmet")]
    PackRequirementUnmet { pack: String, requirement: String },

    /// A pack's `declares` field is a ceiling, not a description — a
    /// pack's own `prompt`/`loop` node can never request a session
    /// profile above what its manifest promises, even when the node's
    /// own YAML asks for more (or asks for nothing and falls back to
    /// the engine's own `edit` default).
    #[error("pack_permissions_ceiling_exceeded")]
    PackPermissionsCeilingExceeded {
        node: NodeId,
        pack: String,
        declared: &'static str,
        effective: &'static str,
    },

    /// A pack's `declares.network: false` closes the network for each node.
    #[error("pack_network_ceiling_exceeded")]
    PackNetworkCeilingExceeded { node: NodeId, pack: String },
}
