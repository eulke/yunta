//! `audited_scope` — which scope a node's own worktree diff is held to.
//! The pure half of the node-level scope check; the imperative call site
//! lives in `run::node_close`.

use yunta_engine::audited_scope;

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
        Some(["src/**".to_string()].as_slice())
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
        Some([].as_slice()),
        "read-only means the worktree comes back untouched"
    );
}

#[test]
fn read_only_outranks_a_declared_scope() {
    let node =
        node("id: plan\nkind: prompt\nprompt: go\npermissions: read-only\nscope: [\"src/**\"]\n");
    assert_eq!(audited_scope(&node), Some([].as_slice()));
}
