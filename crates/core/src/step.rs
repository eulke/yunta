//! What a person or an agent can type next, as data: every command line Yunta
//! tells a reader to type, with the arguments a reader fills in among its data.
//!
//! A surface that offers a step holds it as this data, so a terminal can bind
//! it to a key and a desktop to a button, and `yunta_render::command` writes
//! each one as the line the CLI accepts.

use surco::doc::Depth;

use crate::ids::{NodeId, OptionId, PackRef, RunId, WorkflowName};
use crate::WorkflowShape;

/// A command line a person or an agent types, as data: every one Yunta tells a
/// reader to type, and the git command it points a reader to. Not the CLI's
/// declaration of its commands: this is what a surface offers, which that
/// declaration accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextStep {
    /// `yunta status <run>`, or one node of it with `--node`, at `depth`: a glance,
    /// `--full` or `--dig`.
    Status {
        run: RunId,
        node: Option<NodeId>,
        depth: Depth,
    },
    /// `yunta status needs`: the run waiting on you, and how to answer it.
    Needs,
    /// `yunta status <run> --follow`: the run as it moves, from any process.
    Follow { run: RunId },
    /// `yunta resume <run>`.
    Resume { run: RunId },
    /// `yunta cancel <run>`.
    Cancel { run: RunId },
    /// `yunta close <run>`.
    Close { run: RunId },
    /// `yunta review <run>`: the plan a run waits on, read and answered at a terminal.
    Review { run: RunId },
    /// `yunta open <run>`: the run's page, drawn again and opened.
    Open { run: RunId },
    /// `yunta receipt <run>`.
    Receipt { run: RunId },
    /// `yunta stats <run>`: what a run cost.
    Stats { run: RunId },
    /// `yunta stats --workflow <workflow>`: what a workflow's past runs cost.
    History { workflow: WorkflowName },
    /// `yunta graph <workflow>`: a workflow's graph, as its file declares it.
    Graph { workflow: Arg<String> },
    /// `yunta graph --run <run>`: the graph of the workflow a run froze, each node
    /// labelled with what the run's log derives for it.
    RunGraph { run: RunId },
    /// `yunta verify <run>`.
    Verify { run: RunId },
    /// `yunta resolve-gate <run>`, answered as `choice` says.
    ResolveGate { run: RunId, choice: Choice },
    /// `yunta run <workflow>`.
    Run { workflow: Arg<String> },
    /// `yunta check <workflow>`: a workflow's check, which names each rule it breaks in words.
    Check { workflow: Arg<String> },
    /// `yunta list`: the workflows this repository can run.
    List,
    /// `yunta list --runs`, as far as `reach` reaches.
    ListRuns { reach: Reach },
    /// `yunta schema`: every document it shows.
    Schemas,
    /// `yunta schema <document>`, or with `json` the JSON Schema an editor validates against.
    Schema { document: Arg<String>, json: bool },
    /// `yunta doctor`, with `sessions` a session opened per binding, and with
    /// `full` every check, the healthy ones unfolded.
    Doctor { sessions: bool, full: bool },
    /// `yunta test`: the test cases of the project, or of the one at `dir`.
    Test { dir: Option<String> },
    /// `yunta init`: a repository prepared for yunta.
    Init,
    /// `yunta new <name>`: a new workflow written from the skeleton of `shape`, or
    /// of the shape `new` picks when none is named.
    NewWorkflow {
        name: Arg<String>,
        shape: Option<WorkflowShape>,
    },
    /// `yunta pack add <source>`.
    PackAdd { source: Arg<String> },
    /// `yunta pack update <pack> <ref>`.
    PackUpdate {
        pack: PackRef,
        reference: Arg<String>,
    },
    /// `yunta pack remove <pack>`.
    PackRemove { pack: PackRef },
    /// `yunta pack new <pack>`: the skeleton a new pack starts from.
    PackNew { pack: PackRef },
    /// `yunta pack list`: every installed pack, its content checked against the lock.
    PackList,
    /// `yunta pack audit <pack>`: everything an installed pack holds and asks for.
    PackAudit { pack: PackRef },
    /// `yunta gc`: the disk of runs past their retention, reclaimed; with
    /// `dry_run`, what would be, said and left alone.
    Gc { dry_run: bool },
    /// `yunta --help`: what yunta does, and every command it takes.
    Help,
    /// `git show <commit>`: a commit a run's work is held in, by the id a reader meets it under.
    ShowCommit { commit: String },
}

#[cfg(any(test, feature = "testkit"))]
pub mod samples;

impl NextStep {
    /// `yunta status <run>`: the run at a glance, the step a reader is pointed
    /// to most.
    pub fn status(run: &RunId) -> Self {
        NextStep::Status {
            run: run.clone(),
            node: None,
            depth: Depth::Glance,
        }
    }

    /// The run this step names, where it names one.
    pub fn run(&self) -> Option<&RunId> {
        match self {
            NextStep::Status { run, .. }
            | NextStep::Follow { run }
            | NextStep::Resume { run }
            | NextStep::Cancel { run }
            | NextStep::Close { run }
            | NextStep::Review { run }
            | NextStep::Open { run }
            | NextStep::Receipt { run }
            | NextStep::Stats { run }
            | NextStep::RunGraph { run }
            | NextStep::Verify { run }
            | NextStep::ResolveGate { run, .. } => Some(run),
            NextStep::Needs
            | NextStep::History { .. }
            | NextStep::Graph { .. }
            | NextStep::Run { .. }
            | NextStep::Check { .. }
            | NextStep::List
            | NextStep::ListRuns { .. }
            | NextStep::Schemas
            | NextStep::Schema { .. }
            | NextStep::Doctor { .. }
            | NextStep::Test { .. }
            | NextStep::Init
            | NextStep::NewWorkflow { .. }
            | NextStep::PackAdd { .. }
            | NextStep::PackUpdate { .. }
            | NextStep::PackRemove { .. }
            | NextStep::PackNew { .. }
            | NextStep::PackList
            | NextStep::PackAudit { .. }
            | NextStep::Gc { .. }
            | NextStep::Help
            | NextStep::ShowCommit { .. } => None,
        }
    }
}

/// An argument of a command line: the value a reader types as it is, or a
/// blank they fill in, which `yunta_render::command` writes as the argument's
/// name, `<workflow>` or `<document>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg<T> {
    Given(T),
    Blank,
}

/// How a `resolve-gate` line chooses on a decision: with no option, which puts the
/// decision to the person at a terminal; with an option left blank for the
/// reader to pick off the menu above the line; or with `option`, the words it
/// asks for left blank when `words`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Asked,
    Blank,
    Picked { option: OptionId, words: bool },
}

/// Which runs `yunta list --runs` lists: this repository's, what needs a person
/// first; the closed ones too, with `--full`; or every run on this machine,
/// with `--all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Repository,
    WithClosed,
    Machine,
}
