//! Every command a reader is told to type, written once: each [`NextStep`] as
//! the line a person or an agent types, and each action a run offers as that
//! line beside what it does.
//!
//! Every surface that tells a reader what to type next writes it here: a page, a
//! refusal, a pull request's body, the JSON an agent reads. So a phrase met on
//! one surface is the phrase met on the next, and renaming a subcommand is a
//! change to this file. The CLI's clap declaration is what accepts each line.
//!
//! A line carries what the run is called by — its handle on a line a person
//! reads, its whole id on one an agent copies — and nothing else a reader
//! would have to invent: an option a reader picks stays `<option>`, because an
//! option id printed as an example gets pasted.

use surco::doc::Depth;
use yunta_core::step::{Arg, Choice, NextStep, Reach};
use yunta_core::{NodeId, RunId};
use yunta_replay::frame::{Action, Effect};

/// Who copies a command off a line: a person, who reads a run by its handle,
/// or an agent, which copies its whole id. Not who a page is for: both read
/// the same page, and only how a line names a run differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    Person,
    Agent,
}

impl Reader {
    /// The run as this reader names it on a line: its handle for a person,
    /// its whole id for an agent.
    pub fn name_of(self, run: &RunId) -> &str {
        match self {
            Reader::Person => run.handle(),
            Reader::Agent => run.as_str(),
        }
    }
}

/// The command line a person types for `step`.
pub fn line(step: &NextStep) -> String {
    line_for(step, Reader::Person)
}

/// The command line `reader` types for `step`, the run named the way it
/// copies it.
pub fn line_for(step: &NextStep, reader: Reader) -> String {
    match step {
        NextStep::Status { run, node, depth } => status(reader.name_of(run), node.as_ref(), *depth),
        NextStep::Needs => "yunta status needs".to_string(),
        NextStep::Follow { run } => format!("yunta status {} --follow", reader.name_of(run)),
        NextStep::Resume { run } => format!("yunta resume {}", reader.name_of(run)),
        NextStep::Cancel { run } => format!("yunta cancel {}", reader.name_of(run)),
        NextStep::Close { run } => format!("yunta close {}", reader.name_of(run)),
        NextStep::Review { run } => format!("yunta review {}", reader.name_of(run)),
        NextStep::Open { run } => format!("yunta open {}", reader.name_of(run)),
        NextStep::Receipt { run } => format!("yunta receipt {}", reader.name_of(run)),
        NextStep::Verify { run } => format!("yunta verify {}", reader.name_of(run)),
        NextStep::ResolveGate { run, choice } => resolve_gate(reader.name_of(run), choice),
        NextStep::Run { workflow } => format!("yunta run {}", given(workflow, "<workflow>")),
        NextStep::Check { workflow } => format!("yunta check {}", given(workflow, "<workflow>")),
        NextStep::List => "yunta list".to_string(),
        NextStep::ListRuns { reach } => list_runs(*reach),
        NextStep::Schemas => "yunta schema".to_string(),
        NextStep::Schema { document, json } => {
            format!(
                "yunta schema {}{}",
                given(document, "<document>"),
                flag(*json, " --json")
            )
        }
        NextStep::Doctor { sessions, full } => {
            format!(
                "yunta doctor{}{}",
                flag(*sessions, " --session"),
                flag(*full, " --full")
            )
        }
        NextStep::Test { dir: None } => "yunta test".to_string(),
        NextStep::Test { dir: Some(dir) } => format!("yunta test --dir {dir}"),
        NextStep::Init => "yunta init".to_string(),
        NextStep::PackAdd { source } => format!("yunta pack add {}", given(source, "<source>")),
        NextStep::PackUpdate { pack, reference } => {
            format!("yunta pack update {pack} {}", given(reference, "<ref>"))
        }
        NextStep::PackRemove { pack } => format!("yunta pack remove {pack}"),
        NextStep::PackNew { pack } => format!("yunta pack new {pack}"),
        NextStep::Gc => "yunta gc".to_string(),
        NextStep::Help => "yunta --help".to_string(),
        NextStep::ShowCommit { commit } => format!("git show {commit}"),
    }
}

/// `yunta status`, for the run named `run`, one of its nodes, at a depth.
fn status(run: &str, node: Option<&NodeId>, depth: Depth) -> String {
    let node = node
        .map(|node| format!(" --node {node}"))
        .unwrap_or_default();
    let depth = match depth {
        Depth::Glance => "",
        Depth::Full => " --full",
        Depth::Dig => " --dig",
    };
    format!("yunta status {run}{node}{depth}")
}

/// `yunta resolve-gate` for the run named `run`, choosing as `choice` says.
fn resolve_gate(run: &str, choice: &Choice) -> String {
    match choice {
        Choice::Asked => format!("yunta resolve-gate {run}"),
        Choice::Blank => format!("yunta resolve-gate {run} <option>"),
        Choice::Picked { option, words } => {
            let words = flag(*words, " --text \"<answer>\"");
            format!("yunta resolve-gate {run} {option}{words}")
        }
    }
}

/// `yunta list --runs`, as far as `reach` reaches.
fn list_runs(reach: Reach) -> String {
    match reach {
        Reach::Repository => "yunta list --runs".to_string(),
        Reach::WithClosed => "yunta list --runs --full".to_string(),
        Reach::Machine => "yunta list --runs --all".to_string(),
    }
}

/// `words` when `on`, and nothing otherwise: a flag a line carries or leaves out.
fn flag(on: bool, words: &str) -> &str {
    if on {
        words
    } else {
        ""
    }
}

/// What `effect` does to the run its action names, in the words beside its line.
pub fn effect(effect: Effect) -> &'static str {
    match effect {
        Effect::ContinuesFromLog => "continues it from its log",
        Effect::ContinuesWhereStopped => "continues it where it stopped",
        Effect::WaitsForReset => "waits for the reset, then drives on",
        Effect::HandsBackOnceSettled => "hands it back once that is settled",
        Effect::RetriesOnceTreeChanged => "offers a retry once the run's tree changed",
        Effect::ReadsPlan => "reads its plan, and answers it there",
        Effect::ShowsWhatItWaitsOn => "shows what it is waiting on",
        Effect::Stops => "stops it and everything under it",
        Effect::Closes => "closes it for good",
        Effect::Certifies => "certifies what it did",
        Effect::FindsWhereLogStops => "says where its log stops reading",
    }
}

/// An action as a reader meets it beside a run: its line, as `reader` types
/// it, and what it does.
pub fn said(action: &Action, reader: Reader) -> (String, &'static str) {
    (line_for(&action.step, reader), effect(action.effect))
}

/// An argument as a line writes it: the value given, or the blank `name` a
/// reader fills in.
fn given<'a>(argument: &'a Arg<String>, name: &'a str) -> &'a str {
    match argument {
        Arg::Given(value) => value,
        Arg::Blank => name,
    }
}

#[cfg(test)]
mod tests;
