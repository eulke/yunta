//! What runs from a node on, by rank: what a decision says follows each answer.

use yunta_core::{Because, LeftOut, NodeId, Workflow};

const FLOW: &str = r#"
name: flow
nodes:
  - { id: plan, kind: bash, run: "true" }
  - { id: spec, kind: bash, run: "true", depends_on: [plan] }
  - { id: approve, kind: gate, assignee: lead, depends_on: [spec] }
  - { id: implement, kind: bash, run: "true", depends_on: [approve] }
  - { id: review, kind: bash, run: "true", depends_on: [implement] }
  - { id: conform, kind: bash, run: "true", depends_on: [implement] }
  - { id: lint, kind: bash, run: "true", depends_on: [review, conform] }
  - { id: ship, kind: bash, run: "true", depends_on: [lint] }
  - { id: aside, kind: bash, run: "true", depends_on: [plan] }
"#;

fn flow() -> Workflow {
    yunta_core::yaml::parse(FLOW).expect("a workflow to read ranks from")
}

fn id(name: &str) -> NodeId {
    NodeId::from(name)
}

fn named(ranks: Vec<Vec<NodeId>>) -> Vec<Vec<String>> {
    ranks
        .into_iter()
        .map(|rank| rank.into_iter().map(|node| node.to_string()).collect())
        .collect()
}

/// From a node on, each rank is what waits on the rank before it, in the order the
/// workflow declares them.
#[test]
fn ranks_from_a_node_go_down_what_waits_on_it() {
    assert_eq!(
        named(flow().ranks_from(&[id("implement")], &[])),
        [
            vec!["implement"],
            vec!["review", "conform"],
            vec!["lint"],
            vec!["ship"],
        ]
    );
}

/// A node stands a rank past the furthest of what it waits on, so it never shows
/// before something it waits on.
#[test]
fn a_node_stands_past_the_furthest_of_what_it_waits_on() {
    let yaml = r#"
name: uneven
nodes:
  - { id: a, kind: bash, run: "true" }
  - { id: b, kind: bash, run: "true", depends_on: [a] }
  - { id: c, kind: bash, run: "true", depends_on: [a, b] }
"#;
    let workflow: Workflow = yunta_core::yaml::parse(yaml).expect("a workflow");
    assert_eq!(
        named(workflow.ranks_from(&[id("a")], &[])),
        [vec!["a"], vec!["b"], vec!["c"]]
    );
}

/// What a run left out does not run, and what else waits on goes on from what
/// still leads to it.
#[test]
fn what_the_run_left_out_is_no_rank() {
    let gone = [LeftOut {
        node: id("conform"),
        because: Because::Lacks { lacks: Vec::new() },
    }];
    assert_eq!(
        named(flow().ranks_from(&[id("implement")], &gone)),
        [
            vec!["implement"],
            vec!["review"],
            vec!["lint"],
            vec!["ship"]
        ]
    );
}

/// Between two nodes stands what lies on a way from one to the other, the second
/// last: a branch that leads elsewhere is not on the way.
#[test]
fn ranks_between_two_nodes_hold_what_lies_on_a_way_between_them() {
    assert_eq!(
        named(flow().ranks_between(&id("plan"), &id("approve"), &[])),
        [vec!["plan"], vec!["spec"], vec!["approve"]]
    );
}

/// A node that names nothing downstream of it is one rank, and an empty start is none.
#[test]
fn a_last_node_is_one_rank_and_no_start_is_none() {
    assert_eq!(named(flow().ranks_from(&[id("ship")], &[])), [vec!["ship"]]);
    assert!(flow().ranks_from(&[], &[]).is_empty());
}
