//! Every command a reader is told to type, written once: each [`NextStep`] as
//! the line a person or an agent types, what each does, and each action a run
//! offers as that line beside what it does to the run.
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
use yunta_core::events::GateWaitingPayload;
use yunta_core::step::{Arg, Choice, NextStep, Reach};
use yunta_core::{NodeId, OptionId, RunId, WorkflowShape};
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
        NextStep::Stats { run } => format!("yunta stats {}", reader.name_of(run)),
        NextStep::History { workflow } => format!("yunta stats --workflow {workflow}"),
        NextStep::Graph { workflow } => format!("yunta graph {}", given(workflow, "<workflow>")),
        NextStep::RunGraph { run } => format!("yunta graph --run {}", reader.name_of(run)),
        NextStep::Verify { run } => format!("yunta verify {}", reader.name_of(run)),
        NextStep::ResolveGate { run, choice } => resolve_gate(reader.name_of(run), choice),
        NextStep::Run { workflow } => format!("yunta run {}", given(workflow, "<workflow>")),
        NextStep::Check { workflow } => format!("yunta check {}", given(workflow, "<workflow>")),
        NextStep::List => "yunta list".to_string(),
        NextStep::ListRuns { reach } => list_runs(*reach),
        NextStep::Schemas => "yunta schema".to_string(),
        NextStep::Schema { document, json } => schema(document, *json),
        NextStep::Doctor { sessions, full } => doctor(*sessions, *full),
        NextStep::Test { dir: None } => "yunta test".to_string(),
        NextStep::Test { dir: Some(dir) } => format!("yunta test --dir {dir}"),
        NextStep::Init => "yunta init".to_string(),
        NextStep::NewWorkflow { name, shape } => new_workflow(name, *shape),
        NextStep::PackAdd { source } => format!("yunta pack add {}", given(source, "<source>")),
        NextStep::PackUpdate { pack, reference } => {
            format!("yunta pack update {pack} {}", given(reference, "<ref>"))
        }
        NextStep::PackRemove { pack } => format!("yunta pack remove {pack}"),
        NextStep::PackNew { pack } => format!("yunta pack new {pack}"),
        NextStep::PackList => "yunta pack list".to_string(),
        NextStep::PackAudit { pack } => format!("yunta pack audit {pack}"),
        NextStep::Gc { dry_run } => format!("yunta gc{}", flag(*dry_run, " --dry-run")),
        NextStep::Help => "yunta --help".to_string(),
        NextStep::ShowCommit { commit } => format!("git show {commit}"),
    }
}

/// What `step` does, beside its line where a reader picks one by it: a palette, and
/// help. Not what an action does to the run it names ([`effect`]), which a run's
/// page says.
pub fn does(step: &NextStep) -> &'static str {
    match step {
        NextStep::Status { node, depth, .. } => status_does(node.is_some(), *depth),
        NextStep::Needs => "shows the run waiting on you, and how to answer it",
        NextStep::Follow { .. } => "follows the run as it moves",
        NextStep::Resume { .. } => "continues the run from its log",
        NextStep::Cancel { .. } => "stops the run and everything under it",
        NextStep::Close { .. } => "closes the run for good",
        NextStep::Review { .. } => "reads the plan the run waits on, and answers it there",
        NextStep::Open { .. } => "opens the run's page in a browser",
        NextStep::Receipt { .. } => "writes the run's verified work receipt",
        NextStep::Verify { .. } => "checks the run's event chain and its artifacts",
        NextStep::Stats { .. } => "shows what the run cost",
        NextStep::History { .. } => "shows what the workflow's past runs cost",
        NextStep::Graph { .. } => "draws the workflow's graph",
        NextStep::RunGraph { .. } => "draws the run's graph, each node as its log stands",
        NextStep::ResolveGate { choice, .. } => resolve_gate_does(choice),
        NextStep::Run { .. } => "runs the workflow",
        NextStep::Check { .. } => "checks the workflow, naming each rule it breaks",
        NextStep::List => "lists the workflows this repository can run",
        NextStep::ListRuns { reach } => list_runs_does(*reach),
        NextStep::Schemas => "lists every document yunta shows",
        NextStep::Schema { json, .. } => schema_does(*json),
        NextStep::Doctor { sessions, full } => doctor_does(*sessions, *full),
        NextStep::Test { .. } => "runs the project's workflow test cases",
        NextStep::Init => "prepares the repository for yunta",
        NextStep::NewWorkflow { .. } => "writes a new workflow from a skeleton",
        NextStep::PackAdd { .. } => "installs a pack",
        NextStep::PackUpdate { .. } => "updates a pack to another reference",
        NextStep::PackRemove { .. } => "removes a pack",
        NextStep::PackNew { .. } => "writes the skeleton a new pack starts from",
        NextStep::PackList => "lists the installed packs, each checked against the lock",
        NextStep::PackAudit { .. } => "audits everything a pack holds and asks for",
        NextStep::Gc { dry_run } => gc_does(*dry_run),
        NextStep::Help => "says what yunta does, and every command it takes",
        NextStep::ShowCommit { .. } => "shows the commit the run's work is held in",
    }
}

/// What a `status` step does: one node whole, or the run at `depth`.
fn status_does(one_node: bool, depth: Depth) -> &'static str {
    match (one_node, depth) {
        (true, _) => "shows one node of the run whole",
        (false, Depth::Glance) => "says where the run stands",
        (false, Depth::Full) => "tells the run's whole story",
        (false, Depth::Dig) => "tells the run's story with every trace it keeps",
    }
}

/// What a `resolve-gate` step does, as it chooses.
fn resolve_gate_does(choice: &Choice) -> &'static str {
    match choice {
        Choice::Asked => "puts what the run waits on to you",
        Choice::Blank | Choice::Picked { .. } => "answers the decision the run waits on",
    }
}

/// What listing the runs as far as `reach` does.
fn list_runs_does(reach: Reach) -> &'static str {
    match reach {
        Reach::Repository => "lists this repository's runs, the ones waiting on you first",
        Reach::WithClosed => "lists this repository's runs, the closed ones too",
        Reach::Machine => "lists every run on this machine",
    }
}

/// What a `schema` step does: shows the document, or prints its JSON Schema.
fn schema_does(json: bool) -> &'static str {
    match json {
        true => "prints the JSON Schema an editor validates against",
        false => "shows a document's shape",
    }
}

/// What a `doctor` step does, as far as its flags reach.
fn doctor_does(sessions: bool, full: bool) -> &'static str {
    match (sessions, full) {
        (true, _) => "opens a session on each agent binding",
        (false, true) => "checks this machine, every check unfolded",
        (false, false) => "checks this machine and its agent CLIs",
    }
}

/// What a `gc` step does, or would.
fn gc_does(dry_run: bool) -> &'static str {
    match dry_run {
        true => "says what reclaiming would remove, and removes nothing",
        false => "reclaims the disk of runs past their retention",
    }
}

/// `yunta schema`, for `document`, or the JSON Schema of it with `json`.
fn schema(document: &Arg<String>, json: bool) -> String {
    format!(
        "yunta schema {}{}",
        given(document, "<document>"),
        flag(json, " --json")
    )
}

/// `yunta doctor`, a session opened per binding with `sessions`, every check unfolded
/// with `full`.
fn doctor(sessions: bool, full: bool) -> String {
    format!(
        "yunta doctor{}{}",
        flag(sessions, " --session"),
        flag(full, " --full")
    )
}

/// `yunta new`, for the workflow `name`, from the skeleton of `shape` when one is named.
fn new_workflow(name: &Arg<String>, shape: Option<WorkflowShape>) -> String {
    let shape = shape
        .map(|shape| format!(" --shape {}", shape.label()))
        .unwrap_or_default();
    format!("yunta new {}{shape}", given(name, "<name>"))
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

/// The command that chooses each option `escalation` offers for `run_id`, in the menu's
/// order, with the option it chooses.
pub fn answering(run_id: &RunId, escalation: &GateWaitingPayload) -> Vec<(OptionId, String)> {
    escalation
        .options()
        .iter()
        .map(|option| {
            let step = NextStep::ResolveGate {
                run: run_id.clone(),
                choice: Choice::Picked {
                    option: option.id.clone(),
                    words: option.asks().is_some(),
                },
            };
            (option.id.clone(), line(&step))
        })
        .collect()
}

#[cfg(test)]
mod tests;
