//! The records a failure carries: the attempt an unchanged one repeats, and
//! the scope and the criterion a node or a task asks a person about.

use serde::{Deserialize, Serialize};

use crate::glob::ScopeGlob;

use super::Failure;

/// The attempt that last actually ran on the tree a refused attempt
/// would have run on, and what it failed with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Unchanged {
    pub since: u32,
    pub failure: Box<Failure>,
}

/// One task's request for scope, owed a person's decision: what it asked
/// to be allowed to write, and why, in its session's own words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OwedScope {
    pub task_id: crate::TaskId,
    pub paths: Vec<ScopeGlob>,
    pub reason: String,
}

/// What a node's session asked to be allowed to write, and why, in its
/// own words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RequestedScope {
    pub paths: Vec<ScopeGlob>,
    pub reason: String,
}

/// A task whose declared criterion no work of the plan can pass: the
/// criterion's command, and the reason its session declared it with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UnpassableTask {
    pub task_id: crate::TaskId,
    pub cmd: String,
    pub why: String,
}
