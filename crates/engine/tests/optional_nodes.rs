//! A run leaves out an optional node its project cannot run — and the
//! node only it leads to — the way a mode leaves a node out, and says
//! why.

use yunta_core::events::{run_left_out, EventPayload, NodeEvent, StoredEvent};
use yunta_core::{Because, CommandName, ConfigKey, ConfigLayer, Workflow};
use yunta_engine::{CheckError, NodeStanding, RunReport, RunTerminal};
use yunta_testkit::{Bench, MOCK_CONFIG};

/// A lint the project may not have, the fix only a failing lint leads
/// to, and a node that waits on the lint.
const LINTED: &str = "\
name: linted
nodes:
  - { id: build, kind: bash, run: \"true\" }
  - id: lint
    kind: bash
    run: { command: lint }
    optional: true
    invariant: true
    depends_on: [build]
    on_failure: { goto: fix, max_reroutes: 1 }
  - { id: fix, kind: prompt, runner: executor, prompt: fix it, context: [{ node-output: { node: lint } }] }
  - { id: ship, kind: bash, run: \"true\", depends_on: [lint] }
";

fn started(events: &[StoredEvent]) -> Vec<String> {
    events
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Node(NodeEvent::Started(_)))
            )
        })
        .filter_map(|event| event.node_id.as_ref().map(ToString::to_string))
        .collect()
}

fn lint_command() -> ConfigKey {
    ConfigKey::Command {
        command: CommandName::from("lint"),
    }
}

#[tokio::test]
async fn an_optional_node_the_project_cannot_run_is_left_out_and_its_dependents_wait_on_what_it_waited_on(
) {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(LINTED, "sessions: []\n", MOCK_CONFIG)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    assert_eq!(started(&events), ["build", "ship"]);
    let left: Vec<String> = run_left_out(&events)
        .iter()
        .map(|left| left.node.to_string())
        .collect();
    assert_eq!(left, ["lint", "fix"]);
}

#[tokio::test]
async fn an_optional_node_runs_where_the_project_declares_it() {
    let bench = Bench::new();
    let config = format!("{MOCK_CONFIG}commands:\n  lint: \"true\"\n");
    let RunReport { terminal, .. } = bench
        .run_with_config(LINTED, "sessions: []\n", &config)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    assert_eq!(started(&events), ["build", "lint", "ship"]);
    assert!(run_left_out(&events).is_empty());
}

/// A mode may never drop an invariant; a project may lack what it needs.
#[test]
fn an_invariant_may_be_optional_and_check_does_not_refuse_it() {
    let workflow: Workflow = yunta_core::yaml::parse(LINTED).expect("the workflow parses");
    let errors = yunta_engine::check(&workflow, &ConfigLayer::default(), &|_| None);
    assert!(
        !errors
            .iter()
            .any(|error| matches!(error, CheckError::Unset { .. })),
        "{errors:?}"
    );
}

#[test]
fn a_required_node_with_an_unset_key_is_still_refused() {
    let required = LINTED.replace("    optional: true\n", "");
    let workflow: Workflow = yunta_core::yaml::parse(&required).expect("the workflow parses");
    let errors = yunta_engine::check(&workflow, &ConfigLayer::default(), &|_| None);
    assert!(
        errors.iter().any(|error| matches!(
            error,
            CheckError::Unset { node, key } if node.as_str() == "lint" && *key == lint_command()
        )),
        "{errors:?}"
    );
}

#[tokio::test]
async fn a_left_out_node_stands_apart_from_a_mode_skipped_one() {
    let bench = Bench::new();
    bench
        .run_with_config(LINTED, "sessions: []\n", MOCK_CONFIG)
        .await;
    let workflow: Workflow = yunta_core::yaml::parse(LINTED).expect("the workflow parses");

    let events = bench.events();
    let closed_at = events.last().expect("the run's log").timestamp;
    let frame = yunta_engine::run_frame(&bench.run_id, &workflow, &events, None, closed_at);

    let lint = frame
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "lint")
        .expect("lint is framed");
    assert_eq!(
        lint.state,
        NodeStanding::LeftOut(Because::Lacks {
            lacks: vec![lint_command()]
        })
    );
    assert_eq!((frame.flow.left_out, frame.flow.skipped), (2, 0));
    assert_eq!((frame.flow.done, frame.flow.total), (2, 2));
}

#[test]
fn optional_on_a_node_that_needs_nothing_is_warned() {
    let workflow: Workflow = yunta_core::yaml::parse(
        "name: w\nnodes:\n  - { id: plain, kind: bash, run: \"true\", optional: true }\n",
    )
    .expect("the workflow parses");

    let warnings = yunta_engine::check_warnings(&workflow, &ConfigLayer::default());

    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            yunta_engine::CheckWarning::OptionalNeedsNothing { node } if node.as_str() == "plain"
        )),
        "{warnings:?}"
    );
    let linted: Workflow = yunta_core::yaml::parse(LINTED).expect("the workflow parses");
    assert!(
        !yunta_engine::check_warnings(&linted, &ConfigLayer::default())
            .iter()
            .any(|warning| matches!(
                warning,
                yunta_engine::CheckWarning::OptionalNeedsNothing { .. }
            ))
    );
}
