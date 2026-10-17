//! What an escalation is about: one menu per escalation, with the data its
//! words are written from. The log keeps the data; a surface writes the words.

use serde::{Deserialize, Serialize};

use super::payloads::{DepartureMenu, FailureMenu};
use super::scope_wait::ScopeWait;

/// What an escalation is about, with the data its words are written from.
/// One per escalation, so a menu reads one way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Menu {
    /// A gate an author declared.
    Gate(AuthorGate),
    /// A node's failure and the remedies it offers, or the re-route it exhausted.
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

/// Who an author's gate addresses, as its workflow names them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AuthorGate {
    pub assignee: String,
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

/// Where the forge published the pull request a gate waits on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublishedGate {
    pub url: String,
}

impl Menu {
    /// Whether answering it answers one task of a loop.
    pub fn answers_a_task(&self) -> bool {
        matches!(self, Menu::Scope(_) | Menu::Departure(_))
    }
}
