//! What a commit a run makes says about where it came from: git trailers
//! under its message, naming the run, the node and the task that made
//! it, so `git log --grep` and `git interpret-trailers` find a run's
//! commits on any clone, without the run's log.

use crate::{NodeId, RunId, TaskId};

/// The trailer that names the run a commit came from.
pub const RUN_TRAILER: &str = "Yunta-Run";
/// The trailer that names the node whose work a commit holds.
pub const NODE_TRAILER: &str = "Yunta-Node";
/// The trailer that names the task whose work a commit holds.
pub const TASK_TRAILER: &str = "Yunta-Task";

/// A commit message a run writes: what it says, then a trailer for the
/// run and, when one made it, for the node and the task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMessage {
    said: String,
    run: RunId,
    node: Option<NodeId>,
    task: Option<TaskId>,
}

impl CommitMessage {
    /// A message saying `said`, for a commit `run` makes.
    pub fn new(said: impl Into<String>, run: &RunId) -> Self {
        CommitMessage {
            said: said.into(),
            run: run.clone(),
            node: None,
            task: None,
        }
    }

    /// The same message, naming `node` as what made the commit.
    pub fn node(mut self, node: &NodeId) -> Self {
        self.node = Some(node.clone());
        self
    }

    /// The same message, naming `task` as the work it holds.
    pub fn task(mut self, task: &TaskId) -> Self {
        self.task = Some(task.clone());
        self
    }

    /// The text git records: what it says, a blank line, the trailers.
    pub fn text(&self) -> String {
        let mut text = format!("{}\n\n{RUN_TRAILER}: {}", self.said.trim_end(), self.run);
        if let Some(node) = &self.node {
            text.push_str(&format!("\n{NODE_TRAILER}: {node}"));
        }
        if let Some(task) = &self.task {
            text.push_str(&format!("\n{TASK_TRAILER}: {task}"));
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trailers_close_the_message_in_one_paragraph() {
        let run = RunId::from_static("01JRUN");
        let message = CommitMessage::new("node build: compiles it\n\nWith a body.", &run)
            .node(&NodeId::from("build"))
            .task(&TaskId::from_static("T001"));
        assert_eq!(
            message.text(),
            "node build: compiles it\n\nWith a body.\n\n\
             Yunta-Run: 01JRUN\nYunta-Node: build\nYunta-Task: T001"
        );
    }
}
