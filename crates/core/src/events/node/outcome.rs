//! What a node resolved to when it closed: a fact the engine checked, or the
//! agent's own summary of its work.

use serde::{Deserialize, Serialize};

use crate::events::FindingSeverity;
use crate::ids::{NodeId, OptionId, Responder, RunId};

/// What a node resolved to: a fact the engine checked, or the agent's
/// own summary of its work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum NodeOutcome {
    Fact(EngineFact),
    /// The agent's own summary, or a sentence read from a log.
    Summary(String),
}

/// A fact the engine verified about how a node closed, kept as data: a
/// surface words it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineFact {
    ExitStatus {
        code: i32,
    },
    PullRequestOpened {
        number: u64,
        url: String,
    },
    ChildrenFinished {
        count: usize,
    },
    SucceededFirst {
        node: NodeId,
    },
    TasksDone {
        count: usize,
    },
    ChildRunFinished {
        child: RunId,
    },
    NothingToCompare {
        command: String,
        exit_code: i32,
    },
    NoRegression {
        exit_code: i32,
        reused: bool,
    },
    CoverageMet {
        measured: f64,
        threshold: f64,
    },
    NoFinding {
        severity: FindingSeverity,
    },
    /// A person answered the node's questions.
    QuestionsAnswered,
    /// Nobody was asked: each question says what the work assumes
    /// without an answer.
    QuestionsAssumed,
    /// A gate closed on the option a person chose, which sends the run
    /// nowhere else.
    OptionChosen {
        option: OptionId,
    },
    /// A person approved the gate, and where they did.
    Approved {
        by: Responder,
        on: Approval,
    },
}

/// Where a person approved a gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    /// At the console, on the menu the run asked with.
    Console,
    /// On the forge, by a review.
    Review,
    /// On the forge, by merging the change.
    Merge,
}
