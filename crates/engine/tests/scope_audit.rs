//! `effective_scope` — which scope a node's own worktree diff is held
//! to. The pure half of the node-level scope check; the imperative call
//! site lives in `run::node_close`.

use yunta_engine::{audits, effective_scope, RunState};

/// What `node` is held to on a run whose log granted it nothing and
/// recorded no start.
fn audited_scope(node: &yunta_core::Node) -> Option<Vec<yunta_core::ScopeGlob>> {
    assert_eq!(
        audits(node),
        effective_scope(node, &RunState::default()).is_some()
    );
    effective_scope(node, &RunState::default())
}

fn node(yaml: &str) -> yunta_core::Node {
    yunta_core::yaml::parse(yaml).expect("a node")
}

#[test]
fn a_node_that_declares_nothing_constrains_nothing() {
    let node = node("id: build\nkind: prompt\nprompt: go\n");
    assert_eq!(audited_scope(&node), None);
}

#[test]
fn a_declared_scope_is_what_the_diff_is_held_to() {
    let node = node("id: build\nkind: prompt\nprompt: go\nscope: [\"src/**\"]\n");
    assert_eq!(
        audited_scope(&node),
        Some(vec![yunta_core::ScopeGlob::from("src/**")])
    );
}

#[test]
fn read_only_is_audited_against_nothing_so_any_project_edit_fails_it() {
    // The static check exempts a read-only node from every scope-overlap
    // rule, letting it run beside any other node. That exemption is only
    // sound if the node really does leave the worktree alone.
    let node = node("id: plan\nkind: prompt\nprompt: go\npermissions: read-only\n");
    assert_eq!(
        audited_scope(&node),
        Some(vec![]),
        "read-only means the worktree comes back untouched"
    );
}

#[test]
fn read_only_outranks_a_declared_scope() {
    let node =
        node("id: plan\nkind: prompt\nprompt: go\npermissions: read-only\nscope: [\"src/**\"]\n");
    assert_eq!(audited_scope(&node), Some(vec![]));
}

/// A node scoped to the run is audited like any scoped node; what it may
/// change is what its start recorded, and a start that recorded nothing
/// leaves it nothing.
#[test]
fn a_node_scoped_to_the_run_is_audited_against_what_its_start_recorded() {
    let node = node("id: fix\nkind: prompt\nprompt: go\nscope: run\n");
    assert_eq!(audited_scope(&node), Some(vec![]));
}
