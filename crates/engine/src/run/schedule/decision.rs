//! What the run does next, as the scheduler answers it.

use chrono::{DateTime, Utc};
use yunta_core::events::{Failure, Outlook, PauseReason};
use yunta_core::NodeId;

/// What the run does next. A batch of `Execute` entries is never empty
/// and is always homogeneous — the scheduler never mixes control actions
/// (`Reroute`/`Pause`/`Finish`/`Broken`) into the same step as node
/// executions, so the imperative shell only ever inspects one variant per
/// loop iteration.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// The run owes its lineage's measurement, and none of its nodes
    /// has run: what worked before the invocation started is measured
    /// on the tree the run opens on, once, before anything changes it.
    MeasureBaseline {
        owed: crate::run::baseline::Owed,
    },
    /// One or more independently-ready nodes to execute concurrently —
    /// `(node, attempt)` pairs, in workflow declaration order.
    Execute(Vec<(NodeId, u32)>),
    Reroute {
        from: NodeId,
        to: NodeId,
        attempt: u32,
        max_reroutes: u32,
        cause: Failure,
    },
    Pause {
        reason: PauseReason,
    },
    /// A node failed with no re-route of its own and the run's
    /// `defaults.on_failure` closes the run as failed rather than pausing:
    /// `abort` at the first such failure, `continue` once every node not
    /// blocked behind a failure has run. `failure` is the causing node's
    /// failure, as the log holds it. The imperative shell writes
    /// `run_finished { terminal_state: Failed }` and stops.
    Fail {
        node: NodeId,
        failure: Failure,
    },
    /// A node's re-routes are exhausted — the one pause that
    /// carries real options, since "retry the same
    /// destination once more" and "abort" are both well-defined here
    /// (unlike a plain failure with no `on_failure` at all, which stays
    /// `Pause`). The imperative shell builds the actual
    /// `GateWaitingPayload` from these facts and asks `HumanInteraction`
    /// — this function stays a pure read of the log, no I/O.
    GateExhaustedReroutes {
        node: NodeId,
        goto: NodeId,
        max_reroutes: u32,
        cause: Failure,
    },
    /// A node failed with no re-route of its own, and the run's
    /// `defaults.on_failure` is `pause`: a person decides whether it runs
    /// again. The imperative shell asks — or consumes a decision
    /// `resolve_gate` seeded while the run was parked — and pauses when
    /// nobody is there to ask. A `retry` it records comes back through
    /// `unrerouted` as an `Execute` of `next_attempt`.
    EscalateFailure {
        node: NodeId,
        failure: Failure,
        next_attempt: u32,
        /// What can change the failure, as the log holds it: the remedies a
        /// person may choose from, or why none can change it.
        outlook: Outlook,
    },
    /// A node failed on a limit that resets at `at`, and the run's
    /// `defaults.on_failure` is `pause`: the run pauses until the reset, and
    /// a resume after it runs the node again. A resume detached to that hour
    /// picks the run up.
    WaitUntil {
        node: NodeId,
        at: DateTime<Utc>,
    },
    /// A `kind: gate` node is ready and has never been published —
    /// the imperative shell commits its declared artifacts,
    /// opens the PR (or degrades to console without a forge), and
    /// pauses. One at a time, same reasoning as `GateExhaustedReroutes`:
    /// publishing is I/O this pure function only decides is needed.
    PublishGate {
        node: NodeId,
    },
    /// A `kind: gate` node has already been published and isn't
    /// resolved yet (or its prior approval needs re-checking against
    /// the PR's current head — the SHA-drift case) — the imperative
    /// shell polls the forge and maps approved/changes-requested/
    /// closed/pending. `external_ref` is the forge's own handle, carried
    /// forward from this node's last `gate_waiting` event so the
    /// imperative shell never needs to re-derive it itself.
    PollGate {
        node: NodeId,
        external_ref: String,
    },
    /// An internal gate (`external: None`) is ready (or came
    /// back after its chosen option's re-route completed): the
    /// imperative shell builds the escalation object from
    /// `message`/`options`/`on` and asks `HumanInteraction`. One at a
    /// time, same reasoning as the other gate steps.
    ResolveInternalGate {
        node: NodeId,
    },
    /// A non-gate node the log derives as waiting on the questions it
    /// asked: a `questions_asked` with no `questions_answered` after
    /// it. The imperative shell re-reads the questions from the
    /// artifact and puts them to `HumanInteraction` (`questions_exec`)
    /// — the ONE ask site for first run and resume alike. One at a
    /// time, same reasoning as the gate steps.
    AskQuestions {
        node: NodeId,
    },
    /// A node whose questions were answered and whose close still owes
    /// it a terminal. The answer reopened it exactly where asking left
    /// it, so what is left is the `node_finished` the close deferred —
    /// no session, no new attempt. The ask round, a resume after a
    /// crash between the answer and the terminal, and an answer a
    /// control-plane client pre-seeded all land here.
    FinishAnswered {
        node: NodeId,
    },
    Finish,
    Broken {
        diagnostic: String,
    },
}
