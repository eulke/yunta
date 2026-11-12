//! What a waiting run is waiting on, as the run's document names it for a
//! program: a node parked on a person, the run itself paused, or the engine
//! asking at its terminal.

use yunta_replay::frame::{RunFrame, RunPhase, WaitingOn};

use super::node::NodeWaitJson;

/// What a waiting run is waiting on.
///
/// Told apart by `on`, which each shape carries once, so a reader matches on the shape
/// instead of inferring it from which fields are set: a node parked on a person is a
/// different thing from the run itself stopping, and only one of them has a node to name.
#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum WaitingOnJson {
    /// A node is parked on a person. With several parked at once this
    /// is the first in the workflow's declaration order. `on` is the
    /// kind of wait the node is in, from the node's own shape.
    Node {
        node: String,
        /// What that node waits on, in the same shape a node of
        /// `nodes` carries; it names `on`.
        #[serde(flatten)]
        waiting: NodeWaitJson,
        /// What the run's standing pause recorded, absent for a node
        /// parked while the run itself keeps moving.
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The run itself paused, with the reason it recorded.
    Run { on: &'static str, reason: String },
    /// The engine driving the run is asking a person at its terminal,
    /// where the only answer it takes is given.
    Prompt {
        on: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        node: Option<String>,
        since: chrono::DateTime<chrono::Utc>,
        pid: yunta_core::Pid,
    },
}

impl WaitingOnJson {
    /// The frame's own answer, in the document's shape — or, for a run
    /// that is moving, the question its engine is asking at its terminal.
    /// `None` for a run that is not waiting on anything.
    pub(super) fn of(frame: &RunFrame) -> Option<Self> {
        let RunPhase::Waiting { on } = &frame.phase else {
            let moving = matches!(frame.phase, RunPhase::Created | RunPhase::Running);
            return frame
                .asking
                .as_ref()
                .filter(|_| moving)
                .map(|asking| WaitingOnJson::Prompt {
                    on: "prompt",
                    node: asking.prompt.node.as_ref().map(ToString::to_string),
                    since: asking.prompt.since,
                    pid: asking.pid,
                });
        };
        Some(match on {
            WaitingOn::Node { node, on, reason } => WaitingOnJson::Node {
                node: node.to_string(),
                waiting: NodeWaitJson::of(on),
                reason: reason.as_ref().map(crate::pause::sentence),
            },
            WaitingOn::Run { reason } => WaitingOnJson::Run {
                on: "run",
                reason: crate::pause::sentence(reason),
            },
        })
    }
}
