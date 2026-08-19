use std::collections::HashMap;

use yunta_core::{ConfigLayer, Node, NodeKind, OnFailure, PromptSource, RunnerCandidate, Workflow};
use yunta_engine::{check, CheckError};

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
