//! What an escalation is about: one menu per escalation, with the data its
//! words are written from. The log keeps the data; a surface writes the words.

use serde::{Deserialize, Serialize};

use super::payloads::{DepartureMenu, FailureMenu};
use super::scope_wait::ScopeWait;
use crate::{NodeId, RunId};

/// What an escalation is about, with the data its words are written from.
/// One per escalation, so a menu reads one way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Menu {
    /// A gate an author declared.
    Gate(AuthorGate),
    /// A node's failure and the remedies it offers, or the reroute it exhausted.
    Failure(Box<FailureMenu>),
    /// A departure from the plan a task's session declared.
    Departure(Box<DepartureMenu>),
    /// A scope request of a loop's task, which a person answers.
    Scope(ScopeWait),
    /// The run's token cap, spent.
    Budget(BudgetCap),
    /// A loop's iteration cap, spent.
    LoopCap(LoopCap),
    /// An external gate answered here, because no forge answers.
    Console(ConsoleGate),
    /// An external gate whose pull request a forge reviews.
    Published(PublishedGate),
}

/// Who an author's gate addresses, as its workflow names them, and the
/// gate itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AuthorGate {
    pub assignee: String,
    /// The gate's own node: a run parked on a gate nobody answered names
    /// it here, since the gate has written nothing of its own. Absent on a
    /// log written before gates named themselves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
}

/// `limits.max_tokens_per_run`, and the input and output tokens the log
/// says the run spent against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BudgetCap {
    pub cap: u64,
    pub spent: u64,
}

/// `limits.max_loop_iterations`, and the iterations this invocation ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoopCap {
    pub cap: u32,
    pub ran: u32,
}

/// An external gate answered at the console: its claim says why no forge
/// answers, so it carries nothing more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConsoleGate {}

/// Where the forge published the pull request a gate waits on, and where on
/// its branch the gate put its files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublishedGate {
    pub url: String,
    /// The folder the gate's files are in, from the repository's root. Absent
    /// for a gate that put them at the branch's root: one published before
    /// gates had a folder, and every later lap of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

/// Where on its branch a gate's pull request carries its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateFolder {
    /// A folder of the run's for the gate, from the repository's root: what
    /// the project holds is never written over, and no two gates of a run
    /// write one path.
    Run(String),
    /// The branch's root, where a gate that published before gates had a
    /// folder keeps its files for the rest of its run, beside what its
    /// reviewers already read there.
    Root,
}

impl GateFolder {
    /// The folder `run` gives its gate `node`: `.yunta/gates/<run>/<node>`.
    pub fn of(run: &RunId, node: &NodeId) -> Self {
        GateFolder::Run(format!(".yunta/gates/{run}/{node}"))
    }

    /// Where the gate's file `name` goes, from the repository's root.
    pub fn at(&self, name: &str) -> String {
        match self {
            GateFolder::Run(folder) => format!("{folder}/{name}"),
            GateFolder::Root => name.to_string(),
        }
    }

    /// The folder as the log records it: `None` for the branch's root.
    pub fn recorded(&self) -> Option<&str> {
        match self {
            GateFolder::Run(folder) => Some(folder),
            GateFolder::Root => None,
        }
    }
}

impl Menu {
    /// Whether answering it answers one task of a loop.
    pub fn answers_a_task(&self) -> bool {
        matches!(self, Menu::Scope(_) | Menu::Departure(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gate_s_files_go_in_its_run_s_folder_or_at_the_root() {
        let folder = GateFolder::of(
            &RunId::from_static("run-1"),
            &NodeId::from_static("approve-spec"),
        );
        assert_eq!(
            folder.at("spec.md"),
            ".yunta/gates/run-1/approve-spec/spec.md"
        );
        assert_eq!(folder.recorded(), Some(".yunta/gates/run-1/approve-spec"));
        assert_eq!(GateFolder::Root.at("tasks.md"), "tasks.md");
        assert_eq!(GateFolder::Root.recorded(), None);
    }
}
