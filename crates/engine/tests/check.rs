use std::collections::HashMap;

use yunta_core::{
    ConfigLayer, JoinPolicy, Node, NodeKind, OnFailure, PromptSource, RunnerCandidate, Workflow,
};
use yunta_engine::{check, check_warnings, CheckError, CheckWarning};

fn bash(id: &str, run: &str, depends_on: &[&str]) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Bash {
            run: run.to_string(),
        },
        depends_on: depends_on.iter().map(|&d| d.into()).collect(),
        scope: Vec::new(),
        runner: None,
        artifacts: None,
        hooks: None,
        on_failure: None,
        on_interrupt: None,
        description: None,
        permissions: None,
        network: None,
    }
}

fn prompt(id: &str, runner: &str, depends_on: &[&str]) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Prompt {
            prompt: PromptSource::Inline("do the thing".to_string()),
        },
        depends_on: depends_on.iter().map(|&d| d.into()).collect(),
        scope: Vec::new(),
        runner: Some(runner.to_string()),
        artifacts: None,
        hooks: None,
        on_failure: None,
        on_interrupt: None,
        description: None,
        permissions: None,
        network: None,
    }
}

fn bash_with_scope(id: &str, run: &str, scope: &[&str]) -> Node {
    let mut node = bash(id, run, &[]);
    node.scope = scope.iter().map(|s| s.to_string()).collect();
    node
}

fn parallel(id: &str, join: JoinPolicy, nodes: Vec<Node>) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Parallel { join, nodes },
        depends_on: Vec::new(),
        scope: Vec::new(),
        runner: None,
        artifacts: None,
        hooks: None,
        on_failure: None,
        on_interrupt: None,
        description: None,
        permissions: None,
        network: None,
    }
}

fn workflow(nodes: Vec<Node>) -> Workflow {
    Workflow {
        name: "fixture".to_string(),
        description: None,
        node_defaults: None,
        nodes,
    }
}

fn config_with_runner(role: &str, candidates: usize) -> ConfigLayer {
    let list = (0..candidates)
        .map(|_| RunnerCandidate {
            adapter: "mock".to_string(),
            model: "mock-model".to_string(),
            agent: None,
        })
        .collect();
    ConfigLayer {
        runners: Some(HashMap::from([(role.to_string(), list)])),
        ..Default::default()
    }
}

#[test]
fn a_well_formed_workflow_has_no_errors() {
    let wf = workflow(vec![
        prompt("plan", "planner", &[]),
        bash("lint", "cargo clippy", &["plan"]),
    ]);
    let errors = check(&wf, &config_with_runner("planner", 1));
    assert_eq!(errors, Vec::new());
}

#[test]
fn duplicate_node_id_is_reported() {
    let wf = workflow(vec![bash("a", "true", &[]), bash("a", "false", &[])]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::DuplicateNodeId { id: "a".into() }));
}

#[test]
fn unknown_dependency_is_reported() {
    let wf = workflow(vec![bash("a", "true", &["ghost"])]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::UnknownDependency {
        node: "a".into(),
        unknown: "ghost".into(),
    }));
}

#[test]
fn unknown_goto_target_is_reported() {
    let mut node = bash("a", "true", &[]);
    node.on_failure = Some(OnFailure {
        goto: "ghost".into(),
        max_reroutes: 1,
    });
    let errors = check(&workflow(vec![node]), &ConfigLayer::default());
    assert!(errors.contains(&CheckError::UnknownGotoTarget {
        node: "a".into(),
        target: "ghost".into(),
    }));
}

#[test]
fn depends_on_cycle_is_reported() {
    let wf = workflow(vec![bash("a", "true", &["b"]), bash("b", "true", &["a"])]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CheckError::DependsOnCycle { .. })),
        "expected a DependsOnCycle error, got {errors:?}"
    );
}

#[test]
fn on_failure_goto_never_counts_as_a_depends_on_cycle() {
    // lint depends_on implement; on failure it re-routes to fix-lint,
    // which itself depends_on lint. That is a cycle if goto edges were
    // folded into depends_on — but I14 says re-route edges are a
    // separate set that never relaxes depends_on's acyclicity, so this
    // must check clean.
    let mut lint = bash("lint", "cargo clippy", &["implement"]);
    lint.on_failure = Some(OnFailure {
        goto: "fix-lint".into(),
        max_reroutes: 2,
    });
    let fix_lint = prompt("fix-lint", "mechanical", &["lint"]);
    let implement = prompt("implement", "executor", &[]);

    let mut config = config_with_runner("executor", 1);
    config
        .runners
        .as_mut()
        .unwrap()
        .insert("mechanical".to_string(), vec![]);
    // give mechanical a real candidate too, so this test isolates the
    // cycle question rather than tripping the runner checks.
    config.runners.as_mut().unwrap().insert(
        "mechanical".to_string(),
        vec![RunnerCandidate {
            adapter: "mock".to_string(),
            model: "mock-model".to_string(),
            agent: None,
        }],
    );

    let errors = check(&workflow(vec![implement, lint, fix_lint]), &config);
    assert_eq!(errors, Vec::new());
}

#[test]
fn runner_not_declared_in_config_is_reported() {
    let wf = workflow(vec![prompt("plan", "planner", &[])]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::UnknownRunner {
        node: "plan".into(),
        runner: "planner".to_string(),
    }));
}

#[test]
fn runner_declared_with_zero_candidates_is_reported() {
    let wf = workflow(vec![prompt("plan", "planner", &[])]);
    let errors = check(&wf, &config_with_runner("planner", 0));
    assert!(errors.contains(&CheckError::RunnerHasNoCandidates {
        node: "plan".into(),
        runner: "planner".to_string(),
    }));
}

#[test]
fn a_parallel_group_s_child_id_colliding_with_another_node_is_a_duplicate() {
    // Global uniqueness, not per-group: replay derives node state from a
    // single flat NodeId -> NodeState map, so a child reusing an id in
    // use elsewhere would corrupt derivation, not just read oddly.
    let wf = workflow(vec![
        bash("shared", "true", &[]),
        parallel("group", JoinPolicy::All, vec![bash("shared", "true", &[])]),
    ]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::DuplicateNodeId {
        id: "shared".into()
    }));
}

#[test]
fn two_children_with_overlapping_declared_scope_is_an_error() {
    let wf = workflow(vec![parallel(
        "group",
        JoinPolicy::All,
        vec![
            bash_with_scope("a", "true", &["src/**"]),
            bash_with_scope("b", "true", &["src/lib.rs"]),
        ],
    )]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::OverlappingParallelScope { group, .. } if group.as_str() == "group"
        )),
        "expected an OverlappingParallelScope error, got {errors:?}"
    );
}

#[test]
fn two_children_with_disjoint_declared_scope_has_no_error_or_warning() {
    let wf = workflow(vec![parallel(
        "group",
        JoinPolicy::All,
        vec![
            bash_with_scope("a", "true", &["src/a.rs"]),
            bash_with_scope("b", "true", &["src/b.rs"]),
        ],
    )]);
    assert_eq!(check(&wf, &ConfigLayer::default()), Vec::new());
    assert_eq!(check_warnings(&wf), Vec::new());
}

#[test]
fn two_children_without_declared_scope_produce_a_warning_not_an_error() {
    let wf = workflow(vec![parallel(
        "group",
        JoinPolicy::All,
        vec![bash("a", "true", &[]), bash("b", "true", &[])],
    )]);
    assert_eq!(check(&wf, &ConfigLayer::default()), Vec::new());
    let warnings = check_warnings(&wf);
    assert!(
        warnings.iter().any(|w| matches!(
            w,
            CheckWarning::UndeclaredParallelScope { group } if group.as_str() == "group"
        )),
        "expected an UndeclaredParallelScope warning, got {warnings:?}"
    );
}

#[test]
fn a_single_child_group_never_warns_about_collision() {
    let wf = workflow(vec![parallel(
        "group",
        JoinPolicy::Any,
        vec![bash("a", "true", &[])],
    )]);
    assert_eq!(check(&wf, &ConfigLayer::default()), Vec::new());
    assert_eq!(check_warnings(&wf), Vec::new());
}

#[test]
fn every_error_message_names_its_rule() {
    assert_eq!(
        CheckError::DuplicateNodeId { id: "a".into() }.to_string(),
        "duplicate node id `a`"
    );
    assert_eq!(
        CheckError::UnknownDependency {
            node: "a".into(),
            unknown: "b".into()
        }
        .to_string(),
        "node `a` depends_on unknown node `b`"
    );
    assert_eq!(
        CheckError::UnknownGotoTarget {
            node: "a".into(),
            target: "b".into()
        }
        .to_string(),
        "node `a` on_failure.goto targets unknown node `b`"
    );
    assert_eq!(
        CheckError::DependsOnCycle {
            path: "a -> b -> a".to_string()
        }
        .to_string(),
        "cycle in depends_on: a -> b -> a"
    );
    assert_eq!(
        CheckError::UnknownRunner {
            node: "a".into(),
            runner: "planner".to_string()
        }
        .to_string(),
        "node `a` references runner `planner`, which `runners:` does not define"
    );
    assert_eq!(
        CheckError::RunnerHasNoCandidates {
            node: "a".into(),
            runner: "planner".to_string()
        }
        .to_string(),
        "node `a` references runner `planner`, which `runners:` defines with zero candidates"
    );
}

fn config_with_denied(patterns: &[&str]) -> ConfigLayer {
    ConfigLayer {
        permissions: Some(yunta_core::PermissionsConfig {
            commands: Some(yunta_core::CommandPermissions {
                deny: patterns.iter().map(|s| s.to_string()).collect(),
                allow: vec![],
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn a_bash_command_matching_a_denied_pattern_is_a_check_error_citing_the_rule() {
    let wf = workflow(vec![bash("escalate", "sudo make install", &[])]);
    let errors = check(&wf, &config_with_denied(&["sudo *"]));
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::CommandDenied { node, rule } if node.as_str() == "escalate" && rule.contains("sudo *")
        )),
        "expected a CommandDenied error citing the pattern, got {errors:?}"
    );
}

#[test]
fn a_hook_command_matching_a_denied_pattern_is_a_check_error() {
    let mut node = bash("build", "cargo build", &[]);
    node.hooks = Some(yunta_core::Hooks {
        before: vec![yunta_core::HookStep {
            run: "sudo sysctl -w net.core.x=1".to_string(),
            timeout_seconds: None,
            on_failure: Default::default(),
        }],
        after: vec![],
    });
    let errors = check(&workflow(vec![node]), &config_with_denied(&["sudo *"]));
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::CommandDenied { node, .. } if node.as_str() == "build"
        )),
        "expected a CommandDenied error, got {errors:?}"
    );
}

#[test]
fn a_denied_command_inside_a_parallel_child_is_found_by_the_static_scan() {
    let wf = workflow(vec![parallel(
        "group",
        JoinPolicy::All,
        vec![bash("child", "sudo true", &[])],
    )]);
    let errors = check(&wf, &config_with_denied(&["sudo *"]));
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::CommandDenied { node, .. } if node.as_str() == "child"
        )),
        "expected the scan to recurse into the group, got {errors:?}"
    );
}

#[test]
fn a_command_built_from_a_template_is_not_a_static_error() {
    // The static scan sees the literal YAML text; `{{run.worktree}}` only
    // becomes a real path at runtime — which is exactly where the second
    // enforcement moment catches it (§6.1's two moments).
    let wf = workflow(vec![bash("templated", "ls {{run.worktree}}", &[])]);
    let errors = check(&wf, &config_with_denied(&["sudo *"]));
    assert_eq!(errors, Vec::new());
}

#[test]
fn a_read_only_parallel_child_does_not_count_toward_the_write_collision_warning() {
    // D100's real condition is "two or more children WITH WRITE
    // permissions" — now that `permissions: read-only` exists (T5.7), a
    // read-only child is out of the collision count by declaration.
    let mut reader = bash("reader", "cat notes.md", &[]);
    reader.permissions = Some(yunta_core::NodePermissions::ReadOnly);
    let writer = bash("writer", "touch out.txt", &[]);

    let wf = workflow(vec![parallel(
        "group",
        JoinPolicy::All,
        vec![reader, writer],
    )]);
    let warnings = check_warnings(&wf);
    assert!(
        warnings.is_empty(),
        "one writer alone cannot collide: {warnings:?}"
    );
}
