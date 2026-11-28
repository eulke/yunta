//! The frames a surface test draws, behind the `testkit` feature.
//!
//! Every surface presents the same [`RunFrame`], so a test of one builds
//! a frame and reads the rows it produces. The fields any one test is
//! about are two or three of a dozen, and spelling the other nine at
//! every call site is what lets two tests of the same surface disagree
//! about what an untouched field holds. Here they are filled in once,
//! and a test says only what it is about:
//!
//! ```ignore
//! RunFrame { phase, nodes, ..RunFrame::sample(&RUN) }
//! ```
//!
//! What the clock adds to such a frame is [`RunTimes::sample`]'s, for the
//! frame's own nodes, so a test reads a frame and its times as a surface
//! reads a run's.

use std::time::Duration;

use surco::words::Standing;
use yunta_core::events::TokenUsage;
use yunta_core::{ModeName, NodeId, RunId};

use super::{
    Counter, Declares, Happening, Moment, NodeFrame, NodeStanding, NodeTimes, RunFrame, RunPhase,
    RunTimes,
};
use crate::clock::TimeSpent;

/// How long a built run has gone: half a minute, long enough that a
/// surface says it in seconds.
const GONE: Duration = Duration::from_secs(30);

/// How long a built node has been working, and how long ago it last
/// said anything — a node alive and speaking, which is what a surface
/// draws detail for.
const WORKING: Duration = Duration::from_secs(12);
const SPOKE: Duration = Duration::from_secs(3);

impl RunFrame {
    /// A run of no nodes under `run_id`, running, with nothing spent.
    ///
    /// The base every surface test builds on: fill in the fields the test
    /// is about and take the rest from here.
    pub fn sample(run_id: &RunId) -> RunFrame {
        RunFrame {
            run_id: run_id.clone(),
            workflow: "paced".into(),
            mode: ModeName::default(),
            standing: Standing::Running,
            phase: RunPhase::Running,
            flow: Counter::default(),
            tasks: None,
            reroutes: 0,
            tokens: TokenUsage::default(),
            prior: None,
            nodes: Vec::new(),
            children: Vec::new(),
            degraded: Vec::new(),
            unknown_kinds: Vec::new(),
            blocking_findings: 0,
        }
    }
}

impl NodeFrame {
    /// A `bash` node in `state`, on its first attempt, working and having
    /// spoken a moment ago.
    pub fn sample(id: &NodeId, state: NodeStanding) -> NodeFrame {
        NodeFrame {
            id: id.clone(),
            kind: "bash",
            group: None,
            standing: state.word(),
            state,
            runner: None,
            attempt: Some(1),
            events: 0,
            tokens: TokenUsage::default(),
            artifacts: Vec::new(),
            running_tasks: Vec::new(),
            sessions: Vec::new(),
            activity: Vec::new(),
            reroute: None,
            reroutes: 0,
            decisions: 0,
            question_rounds: 0,
            declares: Declares::default(),
        }
    }
}

impl RunTimes {
    /// What the clock adds to `frame` at an instant half a minute into the
    /// run, with nothing said of it since its last event, nobody asked at a
    /// terminal, and each of its nodes [`NodeTimes::sample`].
    pub fn sample(frame: &RunFrame) -> RunTimes {
        RunTimes {
            elapsed: Some(GONE),
            time: TimeSpent::default(),
            since_last_event: None,
            asking: None,
            nodes: frame
                .nodes
                .iter()
                .map(|node| (node.id.clone(), NodeTimes::sample()))
                .collect(),
        }
    }
}

impl NodeTimes {
    /// A node working a while and having spoken a moment ago, kept no
    /// person waiting.
    pub fn sample() -> NodeTimes {
        NodeTimes {
            elapsed: Some(WORKING),
            last_event_age: Some(SPOKE),
            people: Duration::ZERO,
        }
    }
}

impl Moment {
    /// One moment a surface test says words for.
    ///
    /// The same reason as [`RunFrame::sample`]: a moment is placed by four
    /// fields a test rarely cares about — a seq, an instant, an elapsed, a
    /// node — and spelling all four at every call site is what lets two
    /// tests of the same surface disagree about what an untouched one holds.
    pub fn sample(node: Option<&str>, happening: Happening) -> Moment {
        Moment {
            seq: 1.into(),
            at: chrono::DateTime::UNIX_EPOCH,
            elapsed: Duration::from_secs(3),
            node: node.map(NodeId::from),
            happening,
        }
    }
}
