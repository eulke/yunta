use std::collections::BTreeSet;

use clap::Parser;
use surco::doc::Depth;
use yunta_core::step::{Arg, Choice, NextStep, Reach};
use yunta_core::{NodeId, OptionId, PackRef, RunId};
use yunta_render::command::{line_for, Reader};

use super::Cli;

/// How many kinds of step [`kind`] numbers: one per variant of `NextStep`.
const KINDS: usize = 27;

/// Each kind of step by a number of its own. A kind added to `NextStep` does not
/// build here until it has one, and the test then asks for a sample of it.
fn kind(step: &NextStep) -> usize {
    match step {
        NextStep::Status { .. } => 0,
        NextStep::Follow { .. } => 1,
        NextStep::Resume { .. } => 2,
        NextStep::Cancel { .. } => 3,
        NextStep::Close { .. } => 4,
        NextStep::Review { .. } => 5,
        NextStep::Open { .. } => 6,
        NextStep::Receipt { .. } => 7,
        NextStep::Verify { .. } => 8,
        NextStep::ResolveGate { .. } => 9,
        NextStep::Run { .. } => 10,
        NextStep::Check { .. } => 11,
        NextStep::List => 12,
        NextStep::ListRuns { .. } => 13,
        NextStep::Schemas => 14,
        NextStep::Schema { .. } => 15,
        NextStep::Doctor { .. } => 16,
        NextStep::Test { .. } => 17,
        NextStep::Init => 18,
        NextStep::PackAdd { .. } => 19,
        NextStep::PackUpdate { .. } => 20,
        NextStep::PackRemove { .. } => 21,
        NextStep::PackNew { .. } => 22,
        NextStep::Gc => 23,
        NextStep::Help => 24,
        NextStep::ShowCommit { .. } => 25,
        NextStep::Needs => 26,
    }
}

/// A step of every kind, with each form one takes: given and blank arguments,
/// each answer of a decision, each reach of a listing.
fn every_step() -> Vec<NextStep> {
    [
        steps_of_a_run(),
        decisions(),
        steps_of_a_workflow(),
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

/// The steps about this machine, a project's tests and its packs.
fn steps_of_the_machine() -> Vec<NextStep> {
    let pack = || "acme/lint".parse::<PackRef>().expect("a pack reference");
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
        NextStep::Gc,
        NextStep::Help,
        NextStep::ShowCommit {
            commit: "4f6c0d3".to_string(),
        },
    ]
}

/// `line` as a reader types it: each blank, `<option>` or `"<answer>"`, filled
/// with a value every blank accepts, since a blank is not meant to be typed as it
/// is written.
fn filled(line: &str) -> Vec<String> {
    line.split_whitespace()
        .map(|word| match word.trim_matches('"').starts_with('<') {
            true => "retry".to_string(),
            false => word.to_string(),
        })
        .collect()
}

/// Every line a surface writes is one the CLI accepts once its blanks are
/// filled, for a person and for an agent: a renamed subcommand or flag fails
/// here rather than on a reader's terminal. `git show` is git's, and is held to
/// its own form.
#[test]
fn every_command_line_parses() {
    let steps = every_step();
    let covered: BTreeSet<usize> = steps.iter().map(kind).collect();
    assert_eq!(
        covered,
        (0..KINDS).collect(),
        "every kind of step has a sample here"
    );
    for step in &steps {
        for reader in [Reader::Person, Reader::Agent] {
            let line = line_for(step, reader);
            if let NextStep::ShowCommit { commit } = step {
                assert_eq!(line, format!("git show {commit}"));
                continue;
            }
            let parsed = Cli::try_parse_from(filled(&line));
            let accepted = match &parsed {
                Ok(_) => true,
                Err(error) => error.kind() == clap::error::ErrorKind::DisplayHelp,
            };
            assert!(
                accepted,
                "`{line}` is not a command line yunta reads: {}",
                parsed.err().map(|e| e.to_string()).unwrap_or_default()
            );
        }
    }
}
