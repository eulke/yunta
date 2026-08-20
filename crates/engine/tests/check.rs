use std::collections::HashMap;

use indexmap::IndexMap;
use yunta_core::{
    ConfigLayer, JoinPolicy, ModeInclude, ModeSpec, Node, NodeKind, OnFailure, PromptSource,
    RunnerCandidate, Workflow,
};
use yunta_engine::{check, check_warnings, CheckError, CheckWarning};

fn modes(entries: &[(&str, ModeInclude)]) -> IndexMap<String, ModeSpec> {
    entries
        .iter()
        .map(|(name, include)| {
            (
                (*name).to_string(),
                ModeSpec {
                    include: include.clone(),
                },
            )
        })
        .collect()
}

fn included(ids: &[&str]) -> ModeInclude {
    ModeInclude::Nodes(ids.iter().map(|&id| id.into()).collect())
}

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
        context: Vec::new(),
        invariant: false,
        skills: Vec::new(),
        interactive: None,
        fresh_context: None,
        runners: Vec::new(),
        agent: None,
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
        context: Vec::new(),
        invariant: false,
        skills: Vec::new(),
        interactive: None,
        fresh_context: None,
        runners: Vec::new(),
        agent: None,
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
        context: Vec::new(),
        invariant: false,
        skills: Vec::new(),
        interactive: None,
        fresh_context: None,
        runners: Vec::new(),
        agent: None,
    }
}

fn gate(id: &str, depends_on: &[&str]) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Gate {
            assignee: "reviewer".to_string(),
            message: None,
            options: Vec::new(),
            on: Default::default(),
            external: Some(yunta_core::ExternalGate {
                kind: yunta_core::ForgeKind::PullRequest,
                artifacts: vec!["spec.md".to_string()],
                branch: "{{run.branch}}".to_string(),
            }),
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
        context: Vec::new(),
        invariant: false,
        skills: Vec::new(),
        interactive: None,
        fresh_context: None,
        runners: Vec::new(),
        agent: None,
    }
}

fn config_with_forge() -> ConfigLayer {
    ConfigLayer {
        forge: Some(yunta_core::ForgeConfig {
            github: Some(yunta_core::GitHubForgeConfig {
                repo: "acme/demo".to_string(),
                token_env: "GITHUB_TOKEN".to_string(),
            }),
        }),
        ..Default::default()
    }
}

fn workflow(nodes: Vec<Node>) -> Workflow {
    Workflow {
        name: "fixture".to_string(),
        modes: None,
        description: None,
        inputs: Default::default(),
        node_defaults: None,
        nodes,
        yunta_schema: None,
        on_finish: Vec::new(),
    }
}

fn workflow_with_inputs(
    nodes: Vec<Node>,
    inputs: std::collections::BTreeMap<String, yunta_core::InputSpec>,
) -> Workflow {
    Workflow {
        name: "fixture".to_string(),
        modes: None,
        description: None,
        inputs,
        node_defaults: None,
        nodes,
        yunta_schema: None,
        on_finish: Vec::new(),
    }
}

fn input_spec(yaml: &str) -> yunta_core::InputSpec {
    serde_yaml::from_str(yaml).unwrap()
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
fn external_gate_without_forge_configured_is_rejected() {
    let wf = workflow(vec![gate("approve", &[])]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::ExternalGateWithoutForge {
        node: "approve".into(),
    }));
}

#[test]
fn external_gate_with_forge_configured_is_accepted() {
    let wf = workflow(vec![gate("approve", &[])]);
    let errors = check(&wf, &config_with_forge());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CheckError::ExternalGateWithoutForge { .. })),
        "got: {errors:?}"
    );
}

fn workflow_with_modes(nodes: Vec<Node>, modes: IndexMap<String, ModeSpec>) -> Workflow {
    let mut wf = workflow(nodes);
    wf.modes = Some(modes);
    wf
}

#[test]
fn a_mode_including_all_nodes_has_no_mode_errors() {
    let wf = workflow_with_modes(
        vec![bash("a", "true", &[]), bash("b", "true", &["a"])],
        modes(&[("full", ModeInclude::All)]),
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        !errors.iter().any(|e| matches!(
            e,
            CheckError::ModeReferencesUnknownNode { .. }
                | CheckError::InvariantNodeExcludedFromMode { .. }
                | CheckError::RerouteTargetExcludedFromMode { .. }
        )),
        "got: {errors:?}"
    );
}

#[test]
fn a_mode_referencing_an_unknown_node_is_reported() {
    let wf = workflow_with_modes(
        vec![bash("a", "true", &[])],
        modes(&[("quick", included(&["a", "ghost"]))]),
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::ModeReferencesUnknownNode {
        mode: "quick".into(),
        node: "ghost".into(),
    }));
}

#[test]
fn an_invariant_node_excluded_from_a_mode_is_reported() {
    let mut lint = bash("lint", "cargo clippy", &[]);
    lint.invariant = true;
    let wf = workflow_with_modes(
        vec![lint, bash("ship", "true", &[])],
        modes(&[("quick", included(&["ship"]))]),
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::InvariantNodeExcludedFromMode {
        node: "lint".into(),
        mode: "quick".into(),
    }));
}

#[test]
fn an_invariant_node_present_in_every_mode_has_no_error() {
    let mut lint = bash("lint", "cargo clippy", &[]);
    lint.invariant = true;
    let wf = workflow_with_modes(
        vec![lint, bash("ship", "true", &[])],
        modes(&[
            ("quick", included(&["lint", "ship"])),
            ("full", ModeInclude::All),
        ]),
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CheckError::InvariantNodeExcludedFromMode { .. })),
        "got: {errors:?}"
    );
}

#[test]
fn a_reroute_target_excluded_from_a_mode_is_reported() {
    // Mirrors §10.1's own example: a node in-mode whose on_failure.goto
    // lands on a node that mode leaves out.
    let mut lint = bash("lint", "cargo clippy", &[]);
    lint.on_failure = Some(OnFailure {
        goto: "fix-lint".into(),
        max_reroutes: 2,
    });
    let wf = workflow_with_modes(
        vec![lint, bash("fix-lint", "true", &[])],
        modes(&[("quick", included(&["lint"]))]),
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::RerouteTargetExcludedFromMode {
        mode: "quick".into(),
        node: "lint".into(),
        goto: "fix-lint".into(),
    }));
}

#[test]
fn a_reroute_target_included_in_the_same_mode_has_no_error() {
    let mut lint = bash("lint", "cargo clippy", &[]);
    lint.on_failure = Some(OnFailure {
        goto: "fix-lint".into(),
        max_reroutes: 2,
    });
    let wf = workflow_with_modes(
        vec![lint, bash("fix-lint", "true", &[])],
        modes(&[("quick", included(&["lint", "fix-lint"]))]),
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CheckError::RerouteTargetExcludedFromMode { .. })),
        "got: {errors:?}"
    );
}

#[test]
fn a_re_route_from_a_node_excluded_from_the_mode_is_never_checked() {
    // The failing node itself isn't in "quick" at all — its goto target
    // being missing from the same mode isn't this mode's problem.
    let mut lint = bash("lint", "cargo clippy", &[]);
    lint.on_failure = Some(OnFailure {
        goto: "fix-lint".into(),
        max_reroutes: 2,
    });
    let wf = workflow_with_modes(
        vec![
            lint,
            bash("fix-lint", "true", &[]),
            bash("ship", "true", &[]),
        ],
        modes(&[("quick", included(&["ship"]))]),
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CheckError::RerouteTargetExcludedFromMode { .. })),
        "got: {errors:?}"
    );
}

fn internal_gate(id: &str, options: &[&str], on: &[(&str, &str)]) -> Node {
    let mut node = gate(id, &[]);
    let NodeKind::Gate {
        options: node_options,
        on: node_on,
        external,
        ..
    } = &mut node.kind
    else {
        unreachable!()
    };
    *external = None;
    *node_options = options.iter().map(|o| o.to_string()).collect();
    *node_on = on
        .iter()
        .map(|(option, target)| (option.to_string(), (*target).into()))
        .collect();
    node
}

#[test]
fn an_internal_gate_never_requires_a_forge() {
    let wf = workflow(vec![internal_gate("approve", &["aprobar"], &[])]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CheckError::ExternalGateWithoutForge { .. })),
        "got: {errors:?}"
    );
}

#[test]
fn a_gate_on_mapping_an_undeclared_option_is_reported() {
    let wf = workflow(vec![
        bash("plan", "true", &[]),
        internal_gate("approve", &["aprobar"], &[("ajustar", "plan")]),
    ]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::GateOnUndeclaredOption {
        node: "approve".into(),
        option: "ajustar".to_string(),
    }));
}

#[test]
fn a_gate_on_targeting_an_unknown_node_is_reported() {
    let wf = workflow(vec![internal_gate(
        "approve",
        &["ajustar"],
        &[("ajustar", "ghost")],
    )]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::UnknownGateOptionTarget {
        node: "approve".into(),
        option: "ajustar".to_string(),
        target: "ghost".into(),
    }));
}

#[test]
fn a_mode_excluding_a_gate_option_target_is_reported() {
    // T1.3's own full wording: "un modo que incluye un nodo cuyo `goto`
    // u opción de gate apunta a un nodo excluido" — now checkable since
    // gate options exist in the schema (DI-04).
    let wf = workflow_with_modes(
        vec![
            bash("plan", "true", &[]),
            internal_gate("approve", &["ajustar"], &[("ajustar", "plan")]),
        ],
        modes(&[("quick", included(&["approve"]))]),
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors.contains(&CheckError::RerouteTargetExcludedFromMode {
        mode: "quick".into(),
        node: "approve".into(),
        goto: "plan".into(),
    }));
}

#[test]
fn a_gate_cannot_be_a_parallel_child() {
    let wf = workflow(vec![parallel(
        "group",
        JoinPolicy::All,
        vec![gate("approve", &[])],
    )]);
    let errors = check(&wf, &config_with_forge());
    assert!(errors.contains(&CheckError::GateInsideParallel {
        node: "approve".into(),
        group: "group".into(),
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
    assert_eq!(check_warnings(&wf, &ConfigLayer::default()), Vec::new());
}

#[test]
fn two_children_without_declared_scope_produce_a_warning_not_an_error() {
    let wf = workflow(vec![parallel(
        "group",
        JoinPolicy::All,
        vec![bash("a", "true", &[]), bash("b", "true", &[])],
    )]);
    assert_eq!(check(&wf, &ConfigLayer::default()), Vec::new());
    let warnings = check_warnings(&wf, &ConfigLayer::default());
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
    assert_eq!(check_warnings(&wf, &ConfigLayer::default()), Vec::new());
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
    let warnings = check_warnings(&wf, &ConfigLayer::default());
    assert!(
        warnings.is_empty(),
        "one writer alone cannot collide: {warnings:?}"
    );
}

// --- T6.1: context: (§9) ----------------------------------------------------

#[test]
fn context_on_a_bash_node_is_a_check_error() {
    let mut node = bash("build", "true", &[]);
    node.context = vec![yunta_core::ContextSpec::Command {
        command: "git log".to_string(),
    }];
    let wf = workflow(vec![node]);
    let errors = check(&wf, &ConfigLayer::default());
    match &errors[..] {
        [CheckError::ContextOnUnsupportedNode { node }] => assert_eq!(node.as_str(), "build"),
        other => panic!("expected one ContextOnUnsupportedNode error, got {other:?}"),
    }
}

#[test]
fn context_on_a_prompt_node_is_never_an_error() {
    let mut node = prompt("plan", "planner", &[]);
    node.context = vec![yunta_core::ContextSpec::Ledger {
        ledger: yunta_core::LedgerParams::default(),
    }];
    let wf = workflow(vec![node]);
    let errors = check(
        &wf,
        &ConfigLayer {
            runners: Some(HashMap::from([(
                "planner".to_string(),
                vec![RunnerCandidate {
                    adapter: "mock".to_string(),
                    model: "mock-model".to_string(),
                    agent: None,
                }],
            )])),
            ..Default::default()
        },
    );
    assert_eq!(errors, Vec::new());
}

#[test]
fn a_context_artifact_reference_creates_an_implicit_dependency_cycle_check() {
    // Two nodes that reference each other's artifact purely through
    // `context:` — no explicit `depends_on` at all — must still be
    // caught as a cycle: the implicit edge is exactly as real as a
    // declared one (§9).
    let mut a = prompt("a", "planner", &[]);
    a.context = vec![yunta_core::ContextSpec::Artifact {
        artifact: yunta_core::ArtifactContextRef {
            node: "b".into(),
            name: "b.md".to_string(),
        },
    }];
    let mut b = prompt("b", "planner", &[]);
    b.context = vec![yunta_core::ContextSpec::Artifact {
        artifact: yunta_core::ArtifactContextRef {
            node: "a".into(),
            name: "a.md".to_string(),
        },
    }];
    let wf = workflow(vec![a, b]);
    let errors = check(
        &wf,
        &ConfigLayer {
            runners: Some(HashMap::from([(
                "planner".to_string(),
                vec![RunnerCandidate {
                    adapter: "mock".to_string(),
                    model: "mock-model".to_string(),
                    agent: None,
                }],
            )])),
            ..Default::default()
        },
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CheckError::DependsOnCycle { .. })),
        "a cycle formed purely through context-artifact references must be caught: {errors:?}"
    );
}

// --- T1.5: inputs: (§2.3, D82) --------------------------------------------

#[test]
fn required_true_together_with_a_default_is_a_check_error() {
    let inputs = std::collections::BTreeMap::from([(
        "idea".to_string(),
        input_spec("type: string\nrequired: true\ndefault: x\n"),
    )]);
    let wf = workflow_with_inputs(vec![bash("plan", "true", &[])], inputs);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CheckError::InputRequiredWithDefault { name } if name == "idea")),
        "{errors:?}"
    );
}

#[test]
fn required_false_with_no_default_is_a_check_error() {
    let inputs = std::collections::BTreeMap::from([(
        "idea".to_string(),
        input_spec("type: string\nrequired: false\n"),
    )]);
    let wf = workflow_with_inputs(vec![bash("plan", "true", &[])], inputs);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors
        .iter()
        .any(|e| matches!(e, CheckError::InputOptionalWithoutDefault { name } if name == "idea")));
}

#[test]
fn an_enum_input_with_no_values_is_a_check_error() {
    let inputs = std::collections::BTreeMap::from([(
        "severity".to_string(),
        input_spec("type: enum\nvalues: []\ndefault: x\n"),
    )]);
    let wf = workflow_with_inputs(vec![bash("plan", "true", &[])], inputs);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors
        .iter()
        .any(|e| matches!(e, CheckError::InputEmptyEnumValues { name } if name == "severity")));
}

#[test]
fn a_number_input_with_min_above_max_is_a_check_error() {
    let inputs = std::collections::BTreeMap::from([(
        "n".to_string(),
        input_spec("type: number\nmin: 10\nmax: 1\ndefault: 5\n"),
    )]);
    let wf = workflow_with_inputs(vec![bash("plan", "true", &[])], inputs);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors
        .iter()
        .any(|e| matches!(e, CheckError::InputMinExceedsMax { name, .. } if name == "n")));
}

#[test]
fn a_string_input_with_an_invalid_regex_pattern_is_a_check_error() {
    let inputs = std::collections::BTreeMap::from([(
        "branch".to_string(),
        input_spec("type: string\npattern: \"[\"\ndefault: main\n"),
    )]);
    let wf = workflow_with_inputs(vec![bash("plan", "true", &[])], inputs);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors
        .iter()
        .any(|e| matches!(e, CheckError::InputInvalidPattern { name, .. } if name == "branch")));
}

#[test]
fn a_template_referencing_an_undeclared_input_is_a_check_error() {
    let mut node = bash("plan", "echo {{inputs.idea}}", &[]);
    node.hooks = None;
    let wf = workflow_with_inputs(vec![node], std::collections::BTreeMap::new());
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(
            |e| matches!(e, CheckError::UndeclaredInput { node, name } if node.as_str() == "plan" && name == "idea")
        ),
        "{errors:?}"
    );
}

#[test]
fn a_template_referencing_a_declared_input_passes_check() {
    let inputs = std::collections::BTreeMap::from([(
        "idea".to_string(),
        input_spec("type: string\nrequired: true\n"),
    )]);
    let node = bash("plan", "echo {{inputs.idea}}", &[]);
    let wf = workflow_with_inputs(vec![node], inputs);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CheckError::UndeclaredInput { .. })),
        "{errors:?}"
    );
}

#[test]
fn an_undeclared_input_reference_inside_a_files_context_pattern_is_caught() {
    let mut node = prompt("plan", "planner", &[]);
    node.context = vec![yunta_core::ContextSpec::Files {
        files: vec!["{{inputs.changelog}}".to_string()],
    }];
    let wf = workflow_with_inputs(vec![node], std::collections::BTreeMap::new());
    let errors = check(&wf, &ConfigLayer::default());
    assert!(errors
        .iter()
        .any(|e| matches!(e, CheckError::UndeclaredInput { name, .. } if name == "changelog")));
}

// --- DI-12: top-level fan-out write collision (D100 extended) ----------------

fn config_with_fanout(max_parallel_nodes: u32) -> ConfigLayer {
    serde_yaml::from_str(&format!(
        "defaults:\n  max_parallel_nodes: {max_parallel_nodes}\n"
    ))
    .unwrap()
}

fn scoped(id: &str, scope: &[&str], depends_on: &[&str]) -> Node {
    let mut node = bash(id, "true", depends_on);
    node.scope = scope.iter().map(|s| s.to_string()).collect();
    node
}

#[test]
fn independent_nodes_with_overlapping_scope_error_under_parallel_fanout() {
    let wf = workflow(vec![
        scoped("a", &["src/shared.rs"], &[]),
        scoped("b", &["src/shared.rs"], &[]),
    ]);
    let errors = check(&wf, &config_with_fanout(2));
    assert!(
        errors.iter().any(|e| {
            let text = e.to_string();
            text.contains("`a`") && text.contains("`b`")
        }),
        "the error must cite both nodes: {errors:?}"
    );
}

#[test]
fn sequential_execution_makes_the_same_overlap_legitimate() {
    let wf = workflow(vec![
        scoped("a", &["src/shared.rs"], &[]),
        scoped("b", &["src/shared.rs"], &[]),
    ]);
    assert_eq!(
        check(&wf, &config_with_fanout(1)),
        Vec::new(),
        "with max_parallel_nodes 1, successive writes to one worktree are legitimate"
    );
}

#[test]
fn a_depends_on_chain_is_never_a_fanout_collision() {
    let wf = workflow(vec![
        scoped("a", &["src/shared.rs"], &[]),
        scoped("b", &["src/shared.rs"], &["a"]),
        scoped("c", &["src/shared.rs"], &["b"]),
    ]);
    assert_eq!(
        check(&wf, &config_with_fanout(4)),
        Vec::new(),
        "a dependency path orders the writes — transitively too"
    );
}

#[test]
fn scopeless_independent_writers_warn_once_per_component() {
    let wf = workflow(vec![
        bash("a", "true", &[]),
        bash("b", "true", &[]),
        bash("c", "true", &[]),
    ]);
    assert_eq!(check(&wf, &config_with_fanout(2)), Vec::new());
    let warnings = check_warnings(&wf, &config_with_fanout(2));
    assert_eq!(
        warnings.len(),
        1,
        "one warning per connected component, never per pair: {warnings:?}"
    );
    let text = warnings[0].to_string();
    for id in ["a", "b", "c"] {
        assert!(text.contains(id), "must name `{id}`: {text}");
    }

    // Sequential execution: nothing to warn about.
    assert_eq!(check_warnings(&wf, &config_with_fanout(1)), Vec::new());
}

// --- DI-13: fresh_context / yunta_schema -------------------------------------

#[test]
fn fresh_context_false_is_refused_until_session_resume_exists() {
    let mut node = bash("a", "true", &[]);
    node.fresh_context = Some(false);
    let wf = workflow(vec![node]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors
            .iter()
            .any(|e| e.to_string().contains("fresh_context") && e.to_string().contains("not")),
        "must refuse with an actionable message: {errors:?}"
    );

    // `true` and absent are both fine — every session is fresh today.
    let mut node = bash("a", "true", &[]);
    node.fresh_context = Some(true);
    assert_eq!(
        check(&workflow(vec![node]), &ConfigLayer::default()),
        Vec::new()
    );
}

#[test]
fn a_yunta_schema_range_covering_this_binary_passes_and_one_outside_fails() {
    let mut wf = workflow(vec![bash("a", "true", &[])]);
    wf.yunta_schema = Some(">=1 <2".to_string());
    assert_eq!(check(&wf, &ConfigLayer::default()), Vec::new());

    wf.yunta_schema = Some(">=2".to_string());
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors
            .iter()
            .any(|e| e.to_string().contains("yunta_schema")),
        "an out-of-range requirement must fail check: {errors:?}"
    );

    wf.yunta_schema = Some("not-a-range".to_string());
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors
            .iter()
            .any(|e| e.to_string().contains("yunta_schema")),
        "an unparseable range must fail loudly, never be ignored: {errors:?}"
    );
}

// --- DI-24: distill paths must be declared artifacts -------------------------

#[test]
fn a_distill_path_no_node_declares_producing_fails_check() {
    let yaml = r#"
name: distiller
nodes:
  - id: plan
    kind: bash
    run: "true"
    artifacts:
      produces: [plan.md]
on_finish:
  - distill: [plan.md, ghost.md]
"#;
    let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors
            .iter()
            .any(|e| e.to_string().contains("ghost.md") && e.to_string().contains("distill")),
        "got: {errors:?}"
    );

    let yaml_ok = yaml.replace(", ghost.md", "");
    let wf: Workflow = serde_yaml::from_str(&yaml_ok).unwrap();
    assert_eq!(check(&wf, &ConfigLayer::default()), Vec::new());
}

// --- T9.4: fan-out declaration rules -----------------------------------------

#[test]
fn a_node_with_both_runner_and_runners_is_refused() {
    let yaml = r#"
name: conflicted
nodes:
  - id: review
    kind: prompt
    runner: reviewer
    runners: [reviewer, reviewer-alt]
    prompt: "Audit."
"#;
    let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::BothRunnerAndRunners { node } if node.as_str() == "review"
        )),
        "got: {errors:?}"
    );
}

#[test]
fn a_reroute_targeting_a_fanout_node_is_refused() {
    let yaml = r#"
name: ambiguous
nodes:
  - id: lint
    kind: bash
    run: "true"
    on_failure: { goto: review, max_reroutes: 1 }
  - id: review
    kind: prompt
    runners: [reviewer, reviewer-alt]
    prompt: "Audit."
"#;
    let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::FanOutTarget { target, .. } if target.as_str() == "review"
        )),
        "got: {errors:?}"
    );
}

// --- T9.3: `kind: workflow` static rules -------------------------------------

#[test]
fn runner_bindings_on_a_workflow_node_are_refused() {
    let yaml = r#"
name: composed
nodes:
  - id: qa
    kind: workflow
    use: qa-review
    runner: executor
  - id: qa-fanout
    kind: workflow
    use: qa-review
    runners: [reviewer, reviewer-alt]
  - id: qa-agent
    kind: workflow
    use: qa-review
    agent: benito
"#;
    let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
    let errors = check(&wf, &ConfigLayer::default());
    for (node, field) in [
        ("qa", "runner"),
        ("qa-fanout", "runners"),
        ("qa-agent", "agent"),
    ] {
        assert!(
            errors.iter().any(|e| matches!(
                e,
                CheckError::WorkflowNodeRunnerBinding { node: n, field: f }
                    if n.as_str() == node && *f == field
            )),
            "expected a `{field}` refusal on `{node}`, got: {errors:?}"
        );
    }
}

#[test]
fn inherit_children_of_a_parallel_group_must_declare_scope() {
    let yaml = r#"
name: composed
nodes:
  - id: build
    kind: parallel
    nodes:
      - { id: feat-a, kind: workflow, use: build-feature, isolation: inherit }
      - { id: feat-b, kind: workflow, use: build-feature, isolation: inherit, scope: ["src/b/**"] }
"#;
    let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
    let errors = check(&wf, &ConfigLayer::default());
    // `feat-a` shares the parent's tree with a concurrent sibling and
    // declares nothing — disjointness is unverifiable, refused (§12).
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::InheritChildWithoutScope { node, .. } if node.as_str() == "feat-a"
        )),
        "got: {errors:?}"
    );
    assert!(
        !errors.iter().any(|e| matches!(
            e,
            CheckError::InheritChildWithoutScope { node, .. } if node.as_str() == "feat-b"
        )),
        "feat-b declared its scope, got: {errors:?}"
    );
}

#[test]
fn inherit_children_with_disjoint_scopes_pass_and_overlapping_fail() {
    let disjoint = r#"
name: composed
nodes:
  - id: build
    kind: parallel
    nodes:
      - { id: feat-a, kind: workflow, use: build-feature, isolation: inherit, scope: ["src/a/**"] }
      - { id: feat-b, kind: workflow, use: build-feature, isolation: inherit, scope: ["src/b/**"] }
"#;
    let wf: Workflow = serde_yaml::from_str(disjoint).unwrap();
    assert!(
        check(&wf, &ConfigLayer::default()).is_empty(),
        "disjoint inherit siblings must pass"
    );

    let overlapping = r#"
name: composed
nodes:
  - id: build
    kind: parallel
    nodes:
      - { id: feat-a, kind: workflow, use: build-feature, isolation: inherit, scope: ["src/**"] }
      - { id: feat-b, kind: workflow, use: build-feature, isolation: inherit, scope: ["src/b/**"] }
"#;
    let wf: Workflow = serde_yaml::from_str(overlapping).unwrap();
    assert!(
        check(&wf, &ConfigLayer::default())
            .iter()
            .any(|e| matches!(e, CheckError::OverlappingParallelScope { .. })),
        "overlapping inherit siblings must be refused"
    );
}

// --- T9.3: the composition reference graph (`check_workflow_refs`) -----------

fn catalog_root(files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let catalog = root.path().join(".yunta/workflows");
    std::fs::create_dir_all(&catalog).unwrap();
    for (name, yaml) in files {
        std::fs::write(catalog.join(format!("{name}.yaml")), yaml).unwrap();
    }
    root
}

const LEAF: &str = "name: leaf\nnodes:\n  - { id: work, kind: bash, run: \"true\" }\n";

fn uses(name: &str, child: &str) -> String {
    format!("name: {name}\nnodes:\n  - {{ id: sub, kind: workflow, use: {child} }}\n")
}

#[test]
fn a_missing_composition_reference_is_a_check_error() {
    let root = catalog_root(&[]);
    let wf: Workflow = serde_yaml::from_str(&uses("parent", "ghost")).unwrap();
    let errors = yunta_engine::check_workflow_refs(&wf, &ConfigLayer::default(), root.path());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::WorkflowRefMissing { node, name, .. }
                if node.as_str() == "sub" && name == "ghost"
        )),
        "got: {errors:?}"
    );
}

#[test]
fn a_composition_cycle_is_a_check_error_naming_the_chain() {
    let root = catalog_root(&[
        ("a", uses("a", "b").as_str()),
        ("b", uses("b", "a").as_str()),
    ]);
    let wf: Workflow = serde_yaml::from_str(&uses("parent", "a")).unwrap();
    let errors = yunta_engine::check_workflow_refs(&wf, &ConfigLayer::default(), root.path());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::WorkflowRefCycle { chain } if chain == "a -> b -> a"
        )),
        "got: {errors:?}"
    );
}

#[test]
fn composition_deeper_than_the_limit_is_a_check_error() {
    let root = catalog_root(&[
        ("a", uses("a", "b").as_str()),
        ("b", uses("b", "c").as_str()),
        ("c", LEAF),
    ]);
    let wf: Workflow = serde_yaml::from_str(&uses("parent", "a")).unwrap();
    let config: ConfigLayer = serde_yaml::from_str("limits: { max_workflow_depth: 2 }").unwrap();
    let errors = yunta_engine::check_workflow_refs(&wf, &config, root.path());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::WorkflowRefTooDeep {
                depth: 3,
                max: 2,
                ..
            }
        )),
        "got: {errors:?}"
    );

    // The same graph passes under the reference default (4).
    assert!(
        yunta_engine::check_workflow_refs(&wf, &ConfigLayer::default(), root.path()).is_empty()
    );
}

#[test]
fn a_healthy_composition_graph_passes_check_workflow_refs() {
    let root = catalog_root(&[("a", uses("a", "b").as_str()), ("b", LEAF)]);
    let wf: Workflow = serde_yaml::from_str(&uses("parent", "a")).unwrap();
    assert!(
        yunta_engine::check_workflow_refs(&wf, &ConfigLayer::default(), root.path()).is_empty()
    );
}

// --- DI-17: `context:` allowed on loops --------------------------------------

#[test]
fn context_on_a_loop_node_is_accepted_and_on_bash_still_refused() {
    let looped = r#"
name: ctx
runners: {}
nodes:
  - id: implement
    kind: loop
    until: all_tasks_complete
    prompt: "work"
    context:
      - files: ["notes.md"]
"#;
    let wf: Workflow = serde_yaml::from_str(looped).unwrap();
    assert!(
        !check(&wf, &ConfigLayer::default())
            .iter()
            .any(|e| matches!(e, CheckError::ContextOnUnsupportedNode { .. })),
        "a loop's context is resolved into every task brief (DI-17) — no refusal"
    );

    let bash = r#"
name: ctx
nodes:
  - id: build
    kind: bash
    run: "true"
    context:
      - files: ["notes.md"]
"#;
    let wf: Workflow = serde_yaml::from_str(bash).unwrap();
    assert!(check(&wf, &ConfigLayer::default())
        .iter()
        .any(|e| matches!(e, CheckError::ContextOnUnsupportedNode { .. })));
}

// --- DI-18: minor check rules ------------------------------------------------

#[test]
fn max_parallel_nodes_zero_is_a_check_error() {
    let wf: Workflow =
        serde_yaml::from_str("name: x\nnodes:\n  - { id: a, kind: bash, run: \"true\" }\n")
            .unwrap();
    let config: ConfigLayer = serde_yaml::from_str("defaults: { max_parallel_nodes: 0 }").unwrap();
    assert!(
        check(&wf, &config)
            .iter()
            .any(|e| matches!(e, CheckError::MaxParallelNodesZero)),
        "a 0 would starve every node forever — refused, not clamped silently"
    );
    assert!(!check(&wf, &ConfigLayer::default())
        .iter()
        .any(|e| matches!(e, CheckError::MaxParallelNodesZero)));
}

#[test]
fn a_push_to_the_base_branch_without_a_prior_gate_warns_d48() {
    let config: ConfigLayer = serde_yaml::from_str("project: { base_branch: main }").unwrap();

    // Direct push to the configured base, no gate anywhere upstream.
    let ungated = r#"
name: pushy
nodes:
  - id: build
    kind: bash
    run: "cargo build"
  - id: pr
    kind: bash
    depends_on: [build]
    run: "git push origin main"
"#;
    let wf: Workflow = serde_yaml::from_str(ungated).unwrap();
    assert!(
        check_warnings(&wf, &config)
            .iter()
            .any(|w| matches!(w, CheckWarning::PushToBaseWithoutGate { node, .. } if node.as_str() == "pr")),
        "got: {:?}",
        check_warnings(&wf, &config)
    );

    // The same push behind a gate is deliberate — no warning (D48's own
    // carve-out).
    let gated = r#"
name: pushy
nodes:
  - id: build
    kind: bash
    run: "cargo build"
  - id: ship
    kind: gate
    depends_on: [build]
    assignee: lead
  - id: pr
    kind: bash
    depends_on: [ship]
    run: "git push origin main"
"#;
    let wf: Workflow = serde_yaml::from_str(gated).unwrap();
    assert!(check_warnings(&wf, &config)
        .iter()
        .all(|w| !matches!(w, CheckWarning::PushToBaseWithoutGate { .. })));

    // A push referencing the template form counts the same.
    let templated = r#"
name: pushy
nodes:
  - id: pr
    kind: bash
    run: "git push origin {{project.base_branch}}"
"#;
    let wf: Workflow = serde_yaml::from_str(templated).unwrap();
    assert!(check_warnings(&wf, &config)
        .iter()
        .any(|w| matches!(w, CheckWarning::PushToBaseWithoutGate { .. })));

    // The reference workflow's own `pr` node pushes to {{run.branch}} —
    // clean.
    let run_branch = r#"
name: pushy
nodes:
  - id: pr
    kind: bash
    run: "git push -u origin {{run.branch}}"
"#;
    let wf: Workflow = serde_yaml::from_str(run_branch).unwrap();
    assert!(check_warnings(&wf, &config)
        .iter()
        .all(|w| !matches!(w, CheckWarning::PushToBaseWithoutGate { .. })));
}

// --- DI-20: scope_expansion ceiling in check ---------------------------------

#[test]
fn a_loop_mode_over_the_scope_expansion_ceiling_is_refused() {
    let workflow = r#"
name: expansive
nodes:
  - id: implement
    kind: loop
    until: all_tasks_complete
    prompt: "work"
    scope_expansion:
      mode: rules
      within: ["src/**"]
"#;
    let wf: Workflow = serde_yaml::from_str(workflow).unwrap();
    let ceiling: ConfigLayer =
        serde_yaml::from_str("permissions: { scope_expansion: { max_mode: ask } }").unwrap();
    assert!(
        check(&wf, &ceiling).iter().any(
            |e| matches!(e, CheckError::ScopeExpansionModeOverCeiling { node, .. }
                if node.as_str() == "implement")
        ),
        "got: {:?}",
        check(&wf, &ceiling)
    );

    // Harder than the ceiling is fine; so is everything with no ceiling.
    let deny_node = workflow.replace("mode: rules", "mode: deny");
    let wf_deny: Workflow = serde_yaml::from_str(&deny_node).unwrap();
    assert!(!check(&wf_deny, &ceiling)
        .iter()
        .any(|e| matches!(e, CheckError::ScopeExpansionModeOverCeiling { .. })));
    assert!(!check(&wf, &ConfigLayer::default())
        .iter()
        .any(|e| matches!(e, CheckError::ScopeExpansionModeOverCeiling { .. })));
}
