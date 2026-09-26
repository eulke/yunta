//! One node of a run, as the `--json` document carries it: the word
//! every text surface prints for it, what qualifies that word, where it
//! sits in the graph, and — for a node waiting or one whose session
//! died — the shape a program acts on instead of parsing a sentence.

use yunta_core::events::{Failure, SessionDeath, SessionEnd};
use yunta_engine::{NodeFrame, NodeStanding, NodeState, NodeWait};

use crate::render::state::{NodeDisplay, StateWord};

/// What one waiting node waits on — the same vocabulary the run-level
/// `waiting_on` uses, so the document says "waiting" one way.
#[derive(serde::Serialize)]
#[serde(tag = "on", rename_all = "snake_case")]
pub(crate) enum NodeWaitJson {
    Gate {
        #[serde(skip_serializing_if = "Option::is_none")]
        external_ref: Option<String>,
    },
    Questions {
        asked: Vec<String>,
    },
}

impl NodeWaitJson {
    pub(super) fn of(on: &NodeWait) -> Self {
        match on {
            NodeWait::Gate { external_ref } => NodeWaitJson::Gate {
                external_ref: external_ref.clone(),
            },
            NodeWait::Questions { asked } => NodeWaitJson::Questions {
                asked: asked.as_slice().iter().map(ToString::to_string).collect(),
            },
        }
    }
}

/// One node of the run's frozen workflow, as the document carries it.
///
/// The word and its detail are the two halves every surface shows, so a
/// reader of this document and a reader of `status` are told the same
/// thing about the same node.
#[derive(serde::Serialize)]
pub(crate) struct NodeJson {
    id: String,
    state: StateWord,
    /// What qualifies the word — the outcome, the failure, the attempt
    /// running, the questions it asked. Absent when the word says all
    /// there is.
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    /// The enclosing `parallel` group, absent for a top-level node.
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    /// What this node waits on, for a node that is waiting.
    #[serde(skip_serializing_if = "Option::is_none")]
    waiting_on: Option<NodeWaitJson>,
    /// How this node's session ended, for a node that failed because
    /// one died without ever reporting a terminal event. `detail` says
    /// the same thing in a sentence; this says it in a shape a program
    /// acts on without parsing one.
    #[serde(skip_serializing_if = "Option::is_none")]
    session_death: Option<SessionDeathJson>,
    /// Most recent failed `yunta-run` call in this node's attempt.
    #[serde(skip_serializing_if = "Option::is_none")]
    last_tool_failure: Option<RunToolFailureJson>,
}

#[derive(serde::Serialize)]
pub(crate) struct RunToolFailureJson {
    tool: String,
    cause: yunta_core::events::RunToolFailureCause,
    session_id: String,
}

impl NodeJson {
    pub(super) fn of(node: &NodeFrame, record: Option<&yunta_core::events::NodeRecord>) -> Self {
        let display = NodeDisplay::standing(&node.state);
        NodeJson {
            id: node.id.to_string(),
            state: display.word,
            detail: display.modifier.clone(),
            group: node.group.as_ref().map(ToString::to_string),
            waiting_on: match &node.state {
                NodeStanding::Reached(state) => state.waiting_on().map(NodeWaitJson::of),
                _ => None,
            },
            session_death: match &node.state {
                NodeStanding::Reached(NodeState::Failed {
                    failure: Failure::SessionDied { died },
                    ..
                }) => Some(SessionDeathJson::of(died)),
                _ => None,
            },
            last_tool_failure: record
                .and_then(|record| record.last_tool_failure.as_ref())
                .map(|failed| RunToolFailureJson {
                    tool: failed.tool.name().to_string(),
                    cause: failed.cause,
                    session_id: failed.session_id.to_string(),
                }),
        }
    }
}

/// A session that ended without ever reporting a terminal event.
#[derive(serde::Serialize)]
pub(crate) struct SessionDeathJson {
    adapter: String,
    /// How its process went. Absent for a session with no process of
    /// its own, which has nothing to report about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    exit: Option<SessionExitJson>,
}

/// What the process left behind: how it ended, and the last lines it
/// wrote to stderr, redacted of every value its environment carried.
#[derive(serde::Serialize)]
pub(crate) struct SessionExitJson {
    #[serde(flatten)]
    end: SessionEndJson,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stderr_tail: Vec<String>,
}

/// How the process ended, tagged by `end` so a reader matches on the
/// shape rather than on which field is set.
#[derive(serde::Serialize)]
#[serde(tag = "end", rename_all = "snake_case")]
pub(crate) enum SessionEndJson {
    Code {
        code: i32,
    },
    Signal {
        signal: i32,
    },
    /// A stored end this build does not know — a log a newer binary
    /// wrote.
    Unknown,
}

impl SessionDeathJson {
    fn of(died: &SessionDeath) -> Self {
        SessionDeathJson {
            adapter: died.adapter.to_string(),
            exit: died.exit.as_ref().map(|exit| SessionExitJson {
                end: match exit.end {
                    SessionEnd::Code { code } => SessionEndJson::Code { code },
                    SessionEnd::Signal { signal } => SessionEndJson::Signal { signal },
                    SessionEnd::Unknown => SessionEndJson::Unknown,
                },
                stderr_tail: exit.stderr_tail.clone(),
            }),
        }
    }
}
