//! A step of every kind, with each form one takes: given and blank arguments, each
//! answer of a decision, each reach of a listing, each shape a new workflow starts
//! as. What a test of every line a surface writes, or of what each step does, walks.
//!
//! A kind added to `NextStep` does not build this module's test until the test
//! numbers it, and the test then asks for a sample of it.

use surco::doc::Depth;

use super::{Arg, Choice, NextStep, Reach};
use crate::ids::{NodeId, OptionId, PackName, PackRef, Publisher, RunId, WorkflowName};
use crate::WorkflowShape;

/// A step of every kind, with each form one takes: given and blank arguments,
/// each answer of a decision, each reach of a listing.
pub fn every_step() -> Vec<NextStep> {
    [
        steps_of_a_run(),
        decisions(),
        steps_of_a_workflow(),
        steps_that_show_or_write_a_workflow(),
        steps_of_the_machine(),
    ]
    .concat()
}

/// The steps that name a run, at each depth and for one node.
fn steps_of_a_run() -> Vec<NextStep> {
    let run = || RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    vec![
        NextStep::Status {
            run: run(),
            node: None,
            depth: Depth::Glance,
        },
        NextStep::Status {
            run: run(),
            node: None,
            depth: Depth::Full,
        },
        NextStep::Status {
            run: run(),
            node: None,
            depth: Depth::Dig,
        },
        NextStep::Status {
            run: run(),
            node: Some(NodeId::from_static("build")),
            depth: Depth::Glance,
        },
        NextStep::Needs,
        NextStep::Follow { run: run() },
        NextStep::Resume { run: run() },
        NextStep::Cancel { run: run() },
        NextStep::Close { run: run() },
        NextStep::Review { run: run() },
        NextStep::Open { run: run() },
        NextStep::Receipt { run: run() },
        NextStep::Verify { run: run() },
        NextStep::Stats { run: run() },
        NextStep::RunGraph { run: run() },
    ]
}

/// Each way a `resolve-gate` line chooses on a decision.
fn decisions() -> Vec<NextStep> {
    let run = || RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    vec![
        NextStep::ResolveGate {
            run: run(),
            choice: Choice::Asked,
        },
        NextStep::ResolveGate {
            run: run(),
            choice: Choice::Blank,
        },
        NextStep::ResolveGate {
            run: run(),
            choice: Choice::Picked {
                option: OptionId::from_static("retry"),
                words: false,
            },
        },
        NextStep::ResolveGate {
            run: run(),
            choice: Choice::Picked {
                option: OptionId::from_static("adjust"),
                words: true,
            },
        },
    ]
}

/// The steps about a workflow, the runs listed and the documents' shapes.
fn steps_of_a_workflow() -> Vec<NextStep> {
    vec![
        NextStep::Run {
            workflow: Arg::Given("lint-fix".to_string()),
        },
        NextStep::Run {
            workflow: Arg::Blank,
        },
        NextStep::Check {
            workflow: Arg::Given(".yunta/workflows/lint-fix.yaml".to_string()),
        },
        NextStep::Check {
            workflow: Arg::Blank,
        },
        NextStep::List,
        NextStep::ListRuns {
            reach: Reach::Repository,
        },
        NextStep::ListRuns {
            reach: Reach::WithClosed,
        },
        NextStep::ListRuns {
            reach: Reach::Machine,
        },
        NextStep::Schemas,
        NextStep::Schema {
            document: Arg::Given("tasks".to_string()),
            json: false,
        },
        NextStep::Schema {
            document: Arg::Given("workflow".to_string()),
            json: true,
        },
        NextStep::Schema {
            document: Arg::Blank,
            json: false,
        },
    ]
}

/// The steps that show a workflow, what its past runs cost and its graph, and the one
/// that writes a new one, from a named shape or the one `new` picks.
fn steps_that_show_or_write_a_workflow() -> Vec<NextStep> {
    vec![
        NextStep::History {
            workflow: WorkflowName::from("lint-fix"),
        },
        NextStep::Graph {
            workflow: Arg::Given("lint-fix".to_string()),
        },
        NextStep::Graph {
            workflow: Arg::Blank,
        },
        NextStep::NewWorkflow {
            name: Arg::Given("review".to_string()),
            shape: None,
        },
        NextStep::NewWorkflow {
            name: Arg::Blank,
            shape: Some(WorkflowShape::Tasks),
        },
    ]
}

/// The steps about this machine, a project's tests and its packs.
fn steps_of_the_machine() -> Vec<NextStep> {
    let pack = || {
        PackRef::new(
            Publisher::from_static("acme"),
            PackName::from_static("lint"),
        )
    };
    vec![
        NextStep::Doctor {
            sessions: false,
            full: false,
        },
        NextStep::Doctor {
            sessions: true,
            full: false,
        },
        NextStep::Doctor {
            sessions: false,
            full: true,
        },
        NextStep::Test { dir: None },
        NextStep::Test {
            dir: Some(".".to_string()),
        },
        NextStep::Init,
        NextStep::PackAdd {
            source: Arg::Given("github.com/acme/lint".to_string()),
        },
        NextStep::PackAdd { source: Arg::Blank },
        NextStep::PackUpdate {
            pack: pack(),
            reference: Arg::Given("v2".to_string()),
        },
        NextStep::PackUpdate {
            pack: pack(),
            reference: Arg::Blank,
        },
        NextStep::PackRemove { pack: pack() },
        NextStep::PackNew { pack: pack() },
        NextStep::PackList,
        NextStep::PackAudit { pack: pack() },
        NextStep::Gc { dry_run: false },
        NextStep::Gc { dry_run: true },
        NextStep::Help,
        NextStep::ShowCommit {
            commit: "4f6c0d3".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests;
