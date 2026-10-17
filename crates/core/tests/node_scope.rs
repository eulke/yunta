//! A node's `scope:` — nothing, a list of globs, or `run` — and how the
//! rules read it before any run exists.

use std::path::Path;

use yunta_core::workflow::read::read;
use yunta_core::{NodeScope, ScopeGlob, Workflow};

fn scope_of(line: &str) -> NodeScope {
    let workflow: Workflow = yunta_core::yaml::parse(&format!(
        "name: w\nnodes:\n  - id: a\n    kind: bash\n    run: \"true\"\n{line}"
    ))
    .expect("the workflow parses");
    workflow.nodes[0].scope.clone()
}

#[test]
fn scope_is_globs_or_run_and_round_trips() {
    assert_eq!(scope_of(""), NodeScope::Unscoped);
    assert_eq!(scope_of("    scope: []\n"), NodeScope::Unscoped);
    assert_eq!(
        scope_of("    scope: [\"src/**\"]\n"),
        NodeScope::Globs(vec![ScopeGlob::from("src/**")])
    );
    assert_eq!(scope_of("    scope: run\n"), NodeScope::Run);
    assert!(yunta_core::yaml::parse::<Workflow>(
        "name: w\nnodes:\n  - { id: a, kind: bash, run: \"true\", scope: everything }\n"
    )
    .is_err());

    for scope in [
        NodeScope::Run,
        NodeScope::Globs(vec![ScopeGlob::from("src/**")]),
    ] {
        let text = yunta_core::yaml::to_string(&scope).expect("it serializes");
        assert_eq!(
            yunta_core::yaml::parse::<NodeScope>(&text).expect("it parses back"),
            scope
        );
    }
}

/// What the run changed is known only when the node starts, so before
/// any run a node scoped to it may reach whatever a sibling writes.
#[test]
fn a_parallel_child_scoped_to_the_run_overlaps_any_writing_sibling() {
    let grouped = "name: w\nnodes:\n  - id: fixes\n    kind: parallel\n    nodes:\n      - { id: a, kind: bash, run: \"true\", scope: run }\n      - { id: b, kind: bash, run: \"true\", scope: [\"docs/**\"] }\n";
    let report = read(grouped, Path::new(".yunta/workflows/w.yaml"))
        .expect_err("the two can reach the same files");
    let codes: Vec<String> = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect();
    assert_eq!(codes, ["overlapping-scope"]);
}
