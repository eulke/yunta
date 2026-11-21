//! The blocks a run's own facts are told in: what a failure says beyond
//! its node's row, a node's pane title, what a node hands over and what it
//! runs, a task's proof and its scope, the command each option of a
//! decision names, a quoted refusal, and why a task stopped. Each is said with Surco's components
//! ([`surco::components`]), so every surface draws it the way it draws
//! any other block.
//!
//! What a node hands over, a task's proof and its scope are what a node's
//! pane shows and its printed page prints, each read from the ledgers of
//! the run's state: [`HandsOver`] says each artifact the node declares
//! with where it stands on its open attempt, [`TaskProof`] a task's own
//! row and how it is proved, [`TaskScope`] the paths it may write and
//! which of them its sessions wrote. A task's proof and its scope stand
//! one step under the task's row; where the blocks around them stand is
//! their caller's. A pane opens with [`NodeTitle`], the node, its kind and
//! who works it, which a region holds as its title; [`NodeCommand`] is
//! what a command node or a check runs, which a pane shows where a
//! session's node shows what it hands over. [`FindingsAt`] is the run's
//! findings as the findings' pane shows them, at the one pointed at;
//! [`FindingsThatBlock`] the findings that keep a run that finished from
//! being done, as its receipt's pane lists them under a report.
//! [`HowItStands`] is how a node stands as its own scene says it and its
//! printed page prints it: that it has not run, why its run leaves it out,
//! what a decision holds it on, or what it took and spent and came to.
//! [`WhereItStopped`] is why a run stands with nothing to answer here, as its
//! standstill's pane says it: what it waits on and what moves it again, or
//! where it stopped and why. [`PullRequestReview`] is the pull request a gate
//! waits on, as its scene says it: its link, and how it stands once its forge
//! answered. [`AskedAtItsTerminal`] is a question the process that drives a run
//! asks at its own terminal, as its scene says it: what is asked, who asks and
//! since when, and that nothing here answers it. A run that ended otherwise
//! than finished says its end at the node it names: [`HowItFailed`], the end
//! of what a failed node printed with its kind, its attempt and how long it
//! worked; [`ItsSuccessor`], the run a promotion handed the work to; and
//! [`WhereItsLogBreaks`], where a broken log stops making sense.
//!
//! A card in place on a run's screen rests on what its decision is about,
//! drawn above it: a failure on [`HowItFailed`], which says itself in fewer
//! rows where the card leaves it few; a scope asked for on [`FenceRefused`],
//! the writes the fence turned away that the request asks to make; a
//! departure from the plan on [`Departures`], what the plan decided and what
//! the session says instead; a spent cap on [`SpentAgainstItsCap`].

mod asked_at_its_terminal;
pub mod decision;
mod departures;
pub mod diagnostic;
mod failure;
mod fence_refused;
mod findings_at;
mod findings_that_block;
mod hands_over;
mod how_it_failed;
mod how_it_stands;
mod its_successor;
mod node_command;
mod node_title;
mod pull_request_review;
mod spent_against_its_cap;
pub mod stop;
mod task_proof;
mod task_scope;
mod where_it_stopped;
mod where_its_log_breaks;

pub use asked_at_its_terminal::AskedAtItsTerminal;
pub(crate) use decision::{departure_facts, departure_words};
pub use departures::Departures;
pub use failure::FailureDetail;
pub(crate) use failure::{remedy_words, reroute_words, stuck_words};
pub use fence_refused::FenceRefused;
pub use findings_at::FindingsAt;
pub use findings_that_block::FindingsThatBlock;
pub use hands_over::HandsOver;
pub use how_it_failed::HowItFailed;
pub use how_it_stands::{CameTo, HowItStands};
pub use its_successor::ItsSuccessor;
pub use node_command::NodeCommand;
pub(crate) use node_title::runner_on;
pub use node_title::{runner_of, NodeTitle};
pub use pull_request_review::PullRequestReview;
pub use spent_against_its_cap::SpentAgainstItsCap;
pub(crate) use spent_against_its_cap::{
    ITERATIONS_RUN, ITERATION_CAP, TASKS_READY, TOKENS_SPENT, TOKEN_CAP,
};
pub(crate) use task_proof::task_mark;
pub use task_proof::TaskProof;
pub use task_scope::TaskScope;
pub use where_it_stopped::{AsLeft, WhereItStopped};
pub use where_its_log_breaks::WhereItsLogBreaks;

#[cfg(test)]
mod tests;
