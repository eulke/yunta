//! Every refusal `check` makes about the workflow file says where in the
//! file it is: the node it names and, inside it, the key that says what
//! it refuses — so a surface can quote the line a person fixes.

use yunta_core::yaml::{Location, SourceMap};
use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::{check, CheckError};

/// The workflow `text` declares, checked against `config`, every refusal
/// with the place in `text` it points to.
fn refusals(text: &str, config: &str) -> Vec<(CheckError, Option<(usize, usize)>)> {
    let workflow: Workflow = yunta_core::yaml::parse(text).expect("a workflow that parses");
    let config: ConfigLayer = yunta_core::yaml::parse(config).expect("a config that parses");
    let map = SourceMap::read(text);
    check(&workflow, &config, &|_| None)
        .into_iter()
        .map(|error| {
            let at = error
                .pointer()
                .and_then(|pointer| map.locate_key(&pointer))
                .map(|Location { line, col, .. }| (line, col));
            (error, at)
        })
        .collect()
}

#[test]
fn an_unknown_runner_points_at_its_nodes_runner_key() {
    let text =
        "name: w\nnodes:\n  - id: fix\n    kind: prompt\n    prompt: p\n    runner: implementr\n";
    let found = refusals(text, "{}");
    assert!(
        found.iter().any(
            |(error, at)| matches!(error, CheckError::UnknownRunner { .. }) && *at == Some((6, 5))
        ),
        "{found:?}"
    );
}

#[test]
fn an_unset_command_points_at_the_run_key() {
    let text = "name: w\nnodes:\n  - id: lint\n    kind: bash\n    run: { command: lint }\n";
    let found = refusals(text, "{}");
    assert!(
        found
            .iter()
            .any(|(error, at)| matches!(error, CheckError::Unset { .. }) && *at == Some((5, 5))),
        "{found:?}"
    );
}

#[test]
fn a_cycle_points_at_its_first_node() {
    let text = "name: w\nnodes:\n  - id: a\n    kind: bash\n    run: \"true\"\n    depends_on: [b]\n  - id: b\n    kind: bash\n    run: \"true\"\n    depends_on: [a]\n";
    let found = refusals(text, "{}");
    let (error, at) = found
        .iter()
        .find(|(error, _)| matches!(error, CheckError::DependsOnCycle { .. }))
        .expect("`a` and `b` wait on each other");
    let CheckError::DependsOnCycle { cycle } = error else {
        unreachable!()
    };
    let first_line = match cycle.first().map(|id| id.as_str()) {
        Some("a") => 6,
        Some("b") => 10,
        other => panic!("a cycle of {other:?}"),
    };
    assert_eq!(*at, Some((first_line, 5)), "{found:?}");
}

#[test]
fn a_refusal_about_the_config_points_nowhere_in_the_workflow() {
    assert_eq!(CheckError::MaxParallelNodesZero.pointer(), None);
}
