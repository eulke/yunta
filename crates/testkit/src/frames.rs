//! The frames a surface test draws.
//!
//! Every surface presents the same [`RunFrame`], so a test of one builds
//! a frame and reads the rows it produces. The fields any one test is
//! about are two or three of a dozen, and spelling the other nine at
//! every call site is what lets two tests of the same surface disagree
//! about what an untouched field holds. Here they are filled in once,
//! and a test says only what it is about:
//!
//! ```ignore
//! RunFrame { phase, nodes, ..run_frame(&RUN) }
//! ```

use std::time::Duration;

use yunta_core::events::{TerminalState, TokenUsage};
use yunta_core::{ModeName, NodeId, RunId};
use yunta_engine::{ChildLink, Counter, NodeFrame, NodeStanding, RunFrame, RunPhase};

/// How long a built node has been working, and how long ago it last
/// said anything — a node alive and speaking, which is what a surface
/// draws detail for.
const WORKING: Duration = Duration::from_secs(12);
const SPOKE: Duration = Duration::from_secs(3);

/// A run of no nodes under `run_id`, running, with nothing spent.
///
/// The base every surface test builds on: fill in the fields the test
/// is about and take the rest from here.
pub fn run_frame(run_id: &RunId) -> RunFrame {
    RunFrame {
        run_id: run_id.clone(),
        workflow: "paced".to_string(),
        mode: ModeName::default(),
        phase: RunPhase::Running,
        elapsed: Some(Duration::from_secs(30)),
        flow: Counter::default(),
        tasks: None,
        reroutes: 0,
        tokens: TokenUsage::default(),
        prior: None,
        nodes: Vec::new(),
        children: Vec::new(),
        degraded: Vec::new(),
        unknown_kinds: Vec::new(),
    }
}

/// A `bash` node in `state`, on its first attempt, working and having
/// spoken a moment ago.
pub fn node_frame(id: &NodeId, state: NodeStanding) -> NodeFrame {
    NodeFrame {
        id: id.clone(),
        kind: "bash",
        group: None,
        state,
        runner: None,
        attempt: Some(1),
        elapsed: Some(WORKING),
        last_event_age: Some(SPOKE),
        tokens: TokenUsage::default(),
        artifacts: Vec::new(),
        running_tasks: Vec::new(),
        sessions: Vec::new(),
        activity: Vec::new(),
        reroute: None,
    }
}

/// The link a `kind: workflow` node's child run leaves on the parent's
/// log: `node` is the node that bore it, and `terminal` is how it
/// closed — `None` while the parent's log has no close for it.
pub fn child_link(
    run_id: &RunId,
    node: Option<&NodeId>,
    terminal: Option<TerminalState>,
) -> ChildLink {
    ChildLink {
        run_id: run_id.clone(),
        node: node.cloned(),
        terminal,
    }
}
