use std::collections::BTreeMap;

use indexmap::IndexMap;
use yunta_core::{
    ArtifactSpec, ConfigLayer, JoinPolicy, ModeInclude, ModeName, ModeSpec, Node, NodeKind,
    OnFailure, PromptSource, RunnerCandidate, Workflow,
};
use yunta_engine::{check as check_against, check_warnings, CheckError, CheckWarning};

/// The rules under test here are about the workflow, not about which
/// adapter would run it: these check against a binary that builds none,
/// so a capability nothing declares is a capability nothing can refuse.
fn check(workflow: &yunta_core::Workflow, config: &yunta_core::ConfigLayer) -> Vec<CheckError> {
    check_against(workflow, config, &|_| None)
}

fn modes(entries: &[(&str, ModeInclude)]) -> IndexMap<ModeName, ModeSpec> {
    entries
        .iter()
        .map(|(name, include)| {
            (
                (*name).into(),
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
        runner: Some(runner.into()),
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
        runners: Vec::new(),
        agent: None,
    }
}

fn bash_with_scope(id: &str, run: &str, scope: &[&str]) -> Node {
    let mut node = bash(id, run, &[]);
    node.scope = scope.iter().map(|s| (*s).into()).collect();
    node
}

fn parallel(id: &str, join: JoinPolicy, nodes: Vec<Node>) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Parallel {
            join,
            coordination: yunta_core::Coordination::Independent,
            nodes,
        },
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
                artifacts: vec![yunta_core::ArtifactSpec::Opaque("spec.md".to_string())],
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
        runners: Vec::new(),
        agent: None,
    }
}

fn config_with_forge() -> ConfigLayer {
    ConfigLayer {
        forge: Some(yunta_core::ForgeConfig {
            github: Some(yunta_core::GitHubForgeConfig {
                repo: "acme/demo".parse().unwrap(),
                token_env: "GITHUB_TOKEN".to_string(),
            }),
        }),
        ..Default::default()
    }
}

fn workflow(nodes: Vec<Node>) -> Workflow {
    Workflow {
        name: "fixture".into(),
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
    inputs: std::collections::BTreeMap<yunta_core::InputName, yunta_core::InputSpec>,
) -> Workflow {
    Workflow {
        name: "fixture".into(),
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
    serde_norway::from_str(yaml).unwrap()
}

fn config_with_runner(role: &str, candidates: usize) -> ConfigLayer {
    let list = (0..candidates)
        .map(|_| RunnerCandidate {
            adapter: "mock".into(),
            model: "mock-model".into(),
            agent: None,
        })
        .collect();
    ConfigLayer {
        runners: Some(BTreeMap::from([(role.into(), list)])),
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
    assert_eq!(errors, vec![CheckError::DuplicateNodeId { id: "a".into() }]);
}

#[test]
fn unknown_dependency_is_reported() {
    let wf = workflow(vec![bash("a", "true", &["ghost"])]);
    let errors = check(&wf, &ConfigLayer::default());
    assert_eq!(
        errors,
        vec![CheckError::BrokenReference {
            node: "a".into(),
            field: "depends_on".to_string(),
            target: "ghost".into(),
        }]
    );
}

#[test]
fn unknown_goto_target_is_reported() {
    let mut node = bash("a", "true", &[]);
    node.on_failure = Some(OnFailure {
        goto: "ghost".into(),
        max_reroutes: 1,
    });
    let errors = check(&workflow(vec![node]), &ConfigLayer::default());
    assert_eq!(
        errors,
        vec![CheckError::BrokenReference {
            node: "a".into(),
            field: "on_failure.goto".to_string(),
            target: "ghost".into(),
        }]
    );
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
    // folded into depends_on — but re-route edges are a
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
        .insert("mechanical".into(), vec![]);
    // give mechanical a real candidate too, so this test isolates the
    // cycle question rather than tripping the runner checks.
    config.runners.as_mut().unwrap().insert(
        "mechanical".into(),
        vec![RunnerCandidate {
            adapter: "mock".into(),
            model: "mock-model".into(),
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
    assert_eq!(
        errors,
        vec![CheckError::UnknownRunner {
            node: "plan".into(),
            runner: "planner".into(),
        }]
    );
}

#[test]
fn runner_declared_with_zero_candidates_is_reported() {
    let wf = workflow(vec![prompt("plan", "planner", &[])]);
    let errors = check(&wf, &config_with_runner("planner", 0));
    assert_eq!(
        errors,
        vec![CheckError::RunnerHasNoCandidates {
            node: "plan".into(),
            runner: "planner".into(),
        }]
    );
}

#[test]
fn external_gate_without_forge_configured_is_rejected() {
    let wf = workflow(vec![gate("approve", &[])]);
    let errors = check(&wf, &ConfigLayer::default());
    assert_eq!(
        errors,
        vec![CheckError::ExternalGateWithoutForge {
            node: "approve".into(),
        }]
    );
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

fn workflow_with_modes(nodes: Vec<Node>, modes: IndexMap<ModeName, ModeSpec>) -> Workflow {
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
    assert_eq!(
        errors,
        vec![CheckError::ModeReferencesUnknownNode {
            mode: "quick".into(),
            node: "ghost".into(),
        }]
    );
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
    assert_eq!(
        errors,
        vec![CheckError::InvariantNodeExcludedFromMode {
            node: "lint".into(),
            mode: "quick".into(),
        }]
    );
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
    // A node in-mode whose on_failure.goto
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
    assert_eq!(
        errors,
        vec![CheckError::RerouteTargetExcludedFromMode {
            mode: "quick".into(),
            node: "lint".into(),
            goto: "fix-lint".into(),
        }]
    );
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
    *node_options = options.iter().map(|o| (*o).into()).collect();
    *node_on = on
        .iter()
        .map(|(option, target)| ((*option).into(), (*target).into()))
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
    assert_eq!(
        errors,
        vec![CheckError::GateOnUndeclaredOption {
            node: "approve".into(),
            option: "ajustar".into(),
        }]
    );
}

#[test]
fn a_gate_on_targeting_an_unknown_node_is_reported() {
    let wf = workflow(vec![internal_gate(
        "approve",
        &["ajustar"],
        &[("ajustar", "ghost")],
    )]);
    let errors = check(&wf, &ConfigLayer::default());
    assert_eq!(
        errors,
        vec![CheckError::BrokenReference {
            node: "approve".into(),
            field: "on.ajustar".to_string(),
            target: "ghost".into(),
        }]
    );
}

#[test]
fn a_mode_excluding_a_gate_option_target_is_reported() {
    // "un modo que incluye un nodo cuyo `goto`
    // u opción de gate apunta a un nodo excluido" — now checkable since
    // gate options exist in the schema.
    let wf = workflow_with_modes(
        vec![
            bash("plan", "true", &[]),
            internal_gate("approve", &["ajustar"], &[("ajustar", "plan")]),
        ],
        modes(&[("quick", included(&["approve"]))]),
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert_eq!(
        errors,
        vec![CheckError::RerouteTargetExcludedFromMode {
            mode: "quick".into(),
            node: "approve".into(),
            goto: "plan".into(),
        }]
    );
}

/// A node that asks ends when it asks: whatever depended on the answers
/// belongs to a node that follows it and mounts them as context.
#[test]
fn a_node_that_asks_questions_declares_nothing_else() {
    let mut node = prompt("grill", "planner", &[]);
    node.artifacts = Some(yunta_core::Artifacts {
        produces: vec![
            ArtifactSpec::Interpreted(yunta_core::ArtifactKind::Questions),
            ArtifactSpec::Opaque("brief.md".to_string()),
        ],
    });
    let errors = check(&workflow(vec![node]), &config_with_runner("planner", 1));
    assert_eq!(
        errors,
        vec![CheckError::QuestionsAlongsideOtherArtifacts {
            node: "grill".into(),
            others: vec![ArtifactSpec::Opaque("brief.md".to_string())],
        }]
    );
}

#[test]
fn the_refusal_spells_the_split_and_how_to_read_the_answers() {
    let refusal = CheckError::QuestionsAlongsideOtherArtifacts {
        node: "grill".into(),
        others: vec![ArtifactSpec::Opaque("brief.md".to_string())],
    }
    .to_string();
    assert!(
        refusal.contains("`brief.md`"),
        "the refusal names what has to move: {refusal}"
    );
    assert!(
        refusal.contains("a node that follows it"),
        "the refusal says where it goes: {refusal}"
    );
    assert!(
        refusal.contains("name: questions.answers.yaml"),
        "the refusal spells how the next node reads the answers: {refusal}"
    );
}

/// Only a `prompt` node holds the session that hands questions over and
/// the close that waits on them.
#[test]
fn only_a_prompt_node_asks() {
    let mut node = bash("ask", "echo hi", &[]);
    node.artifacts = Some(yunta_core::Artifacts {
        produces: vec![ArtifactSpec::Interpreted(
            yunta_core::ArtifactKind::Questions,
        )],
    });
    let errors = check(&workflow(vec![node]), &ConfigLayer::default());
    assert_eq!(
        errors,
        vec![CheckError::QuestionsOnKind {
            node: "ask".into(),
            kind: "bash",
        }]
    );
}

/// The scheduler asks one top-level node at a time, so a group's child
/// would wait forever.
#[test]
fn a_node_that_asks_is_refused_inside_a_parallel_group() {
    let mut child = prompt("grill", "planner", &[]);
    child.artifacts = Some(yunta_core::Artifacts {
        produces: vec![ArtifactSpec::Interpreted(
            yunta_core::ArtifactKind::Questions,
        )],
    });
    let wf = workflow(vec![parallel("group", JoinPolicy::All, vec![child])]);
    let errors = check(&wf, &config_with_runner("planner", 1));
    assert_eq!(
        errors,
        vec![CheckError::QuestionsInsideParallel {
            node: "grill".into(),
            group: "group".into(),
        }]
    );
}

#[test]
fn a_gate_cannot_be_a_parallel_child() {
    let wf = workflow(vec![parallel(
        "group",
        JoinPolicy::All,
        vec![gate("approve", &[])],
    )]);
    let errors = check(&wf, &config_with_forge());
    assert_eq!(
        errors,
        vec![CheckError::GateInsideParallel {
            node: "approve".into(),
            group: "group".into(),
        }]
    );
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
    assert_eq!(
        errors,
        vec![CheckError::DuplicateNodeId {
            id: "shared".into()
        }]
    );
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
        CheckError::BrokenReference {
            node: "a".into(),
            field: "depends_on".to_string(),
            target: "b".into()
        }
        .to_string(),
        "node `a`: `depends_on` references unknown node `b`"
    );
    assert_eq!(
        CheckError::BrokenReference {
            node: "a".into(),
            field: "on_failure.goto".to_string(),
            target: "b".into()
        }
        .to_string(),
        "node `a`: `on_failure.goto` references unknown node `b`"
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
            runner: "planner".into()
        }
        .to_string(),
        "node `a` references runner `planner`, which `runners:` does not define"
    );
    assert_eq!(
        CheckError::RunnerHasNoCandidates {
            node: "a".into(),
            runner: "planner".into()
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
    // enforcement moment (runtime command validation) catches it.
    let wf = workflow(vec![bash("templated", "ls {{run.worktree}}", &[])]);
    let errors = check(&wf, &config_with_denied(&["sudo *"]));
    assert_eq!(errors, Vec::new());
}

#[test]
fn a_read_only_parallel_child_does_not_count_toward_the_write_collision_warning() {
    // The real condition is "two or more children WITH WRITE
    // permissions" — now that `permissions: read-only` exists, a
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

// --- context: -----------------------------------------------------------------

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
    node.context = vec![yunta_core::ContextSpec::Tasks {
        tasks: yunta_core::TasksParams::default(),
    }];
    let wf = workflow(vec![node]);
    let errors = check(
        &wf,
        &ConfigLayer {
            runners: Some(BTreeMap::from([(
                "planner".into(),
                vec![RunnerCandidate {
                    adapter: "mock".into(),
                    model: "mock-model".into(),
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
    // declared one.
    let mut a = prompt("a", "planner", &[]);
    a.context = vec![yunta_core::ContextSpec::Artifact {
        artifact: yunta_core::ArtifactContextRef {
            node: Some("b".into()),
            id: yunta_core::ArtifactRefId::Name {
                name: "b.md".into(),
            },
        },
    }];
    let mut b = prompt("b", "planner", &[]);
    b.context = vec![yunta_core::ContextSpec::Artifact {
        artifact: yunta_core::ArtifactContextRef {
            node: Some("a".into()),
            id: yunta_core::ArtifactRefId::Name {
                name: "a.md".into(),
            },
        },
    }];
    let wf = workflow(vec![a, b]);
    let errors = check(
        &wf,
        &ConfigLayer {
            runners: Some(BTreeMap::from([(
                "planner".into(),
                vec![RunnerCandidate {
                    adapter: "mock".into(),
                    model: "mock-model".into(),
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

// --- inputs: --------------------------------------------------------------

#[test]
fn an_enum_input_with_no_values_is_a_check_error() {
    let inputs = std::collections::BTreeMap::from([(
        "severity".into(),
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
        "n".into(),
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
        "branch".into(),
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
        "idea".into(),
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

// --- top-level fan-out write collision ---------------------------------------

fn config_with_fanout(max_parallel_nodes: u32) -> ConfigLayer {
    serde_norway::from_str(&format!(
        "defaults:\n  max_parallel_nodes: {max_parallel_nodes}\n"
    ))
    .unwrap()
}

fn scoped(id: &str, scope: &[&str], depends_on: &[&str]) -> Node {
    let mut node = bash(id, "true", depends_on);
    node.scope = scope.iter().map(|s| (*s).into()).collect();
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
    assert_eq!(
        warnings[0],
        CheckWarning::UndeclaredFanOutScope {
            nodes: "`a`, `b`, `c`".to_string()
        }
    );

    // Sequential execution: nothing to warn about.
    assert_eq!(check_warnings(&wf, &config_with_fanout(1)), Vec::new());
}

// --- yunta_schema ------------------------------------------------------------

#[test]
fn a_yunta_schema_range_covering_this_binary_passes_and_one_outside_fails() {
    let mut wf = workflow(vec![bash("a", "true", &[])]);
    wf.yunta_schema = Some(">=1 <2".into());
    assert_eq!(check(&wf, &ConfigLayer::default()), Vec::new());

    wf.yunta_schema = Some(">=2".into());
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(
            |e| matches!(e, CheckError::YuntaSchemaOutside { range, .. } if range.as_str() == ">=2")
                && e.to_string().contains("yunta_schema")
        ),
        "an out-of-range requirement must fail check: {errors:?}"
    );
}

/// A range nobody can read never reaches `check`: it is refused where
/// the workflow is read, naming the comparator that stopped it.
#[test]
fn a_yunta_schema_range_that_does_not_parse_is_refused_at_read() {
    let error = "not-a-range"
        .parse::<yunta_core::SchemaRange>()
        .expect_err("a range with no version number should not parse");
    assert_eq!(
        error,
        yunta_core::SchemaRangeError::NoVersion {
            comparator: "not-a-range".to_string()
        }
    );
}

// --- distill paths must be declared artifacts ---------------------------------

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
  - distill: [{ node: plan, name: plan.md }, { node: plan, name: ghost.md }]
"#;
    let wf: Workflow = serde_norway::from_str(yaml).unwrap();
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors
            .iter()
            .any(|e| e.to_string().contains("ghost.md") && e.to_string().contains("distill")),
        "got: {errors:?}"
    );

    let yaml_ok = yaml.replace(", { node: plan, name: ghost.md }", "");
    let wf: Workflow = serde_norway::from_str(&yaml_ok).unwrap();
    assert_eq!(check(&wf, &ConfigLayer::default()), Vec::new());
}

// --- fan-out declaration rules -------------------------------------------------

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
    let wf: Workflow = serde_norway::from_str(yaml).unwrap();
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
    let wf: Workflow = serde_norway::from_str(yaml).unwrap();
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::FanOutTarget { target, .. } if target.as_str() == "review"
        )),
        "got: {errors:?}"
    );
}

// --- `kind: workflow` static rules ----------------------------------------------

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
    let wf: Workflow = serde_norway::from_str(yaml).unwrap();
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
    let wf: Workflow = serde_norway::from_str(yaml).unwrap();
    let errors = check(&wf, &ConfigLayer::default());
    // `feat-a` shares the parent's tree with a concurrent sibling and
    // declares nothing — disjointness is unverifiable, so it is refused.
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
    let wf: Workflow = serde_norway::from_str(disjoint).unwrap();
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
    let wf: Workflow = serde_norway::from_str(overlapping).unwrap();
    assert!(
        check(&wf, &ConfigLayer::default())
            .iter()
            .any(|e| matches!(e, CheckError::OverlappingParallelScope { .. })),
        "overlapping inherit siblings must be refused"
    );
}

// --- the composition reference graph (`check_workflow_refs`) -----------------

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
    let wf: Workflow = serde_norway::from_str(&uses("parent", "ghost")).unwrap();
    let errors = yunta_engine::check_workflow_refs(
        &wf,
        &ConfigLayer::default(),
        root.path(),
        &yunta_engine::WorkflowOrigin::Repo,
    );
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
    let wf: Workflow = serde_norway::from_str(&uses("parent", "a")).unwrap();
    let errors = yunta_engine::check_workflow_refs(
        &wf,
        &ConfigLayer::default(),
        root.path(),
        &yunta_engine::WorkflowOrigin::Repo,
    );
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
    let wf: Workflow = serde_norway::from_str(&uses("parent", "a")).unwrap();
    let config: ConfigLayer = serde_norway::from_str("limits: { max_workflow_depth: 2 }").unwrap();
    let errors = yunta_engine::check_workflow_refs(
        &wf,
        &config,
        root.path(),
        &yunta_engine::WorkflowOrigin::Repo,
    );
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
    assert!(yunta_engine::check_workflow_refs(
        &wf,
        &ConfigLayer::default(),
        root.path(),
        &yunta_engine::WorkflowOrigin::Repo
    )
    .is_empty());
}

#[test]
fn a_healthy_composition_graph_passes_check_workflow_refs() {
    let root = catalog_root(&[("a", uses("a", "b").as_str()), ("b", LEAF)]);
    let wf: Workflow = serde_norway::from_str(&uses("parent", "a")).unwrap();
    assert!(yunta_engine::check_workflow_refs(
        &wf,
        &ConfigLayer::default(),
        root.path(),
        &yunta_engine::WorkflowOrigin::Repo
    )
    .is_empty());
}

// --- `context:` allowed on loops ------------------------------------------------

#[test]
fn context_on_a_loop_node_is_accepted_and_on_bash_still_refused() {
    let looped = r#"
name: ctx
nodes:
  - id: implement
    kind: loop
    until: all_tasks_complete
    prompt: "work"
    context:
      - files: ["notes.md"]
"#;
    let wf: Workflow = serde_norway::from_str(looped).unwrap();
    assert!(
        !check(&wf, &ConfigLayer::default())
            .iter()
            .any(|e| matches!(e, CheckError::ContextOnUnsupportedNode { .. })),
        "a loop's context is resolved into every task brief — no refusal"
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
    let wf: Workflow = serde_norway::from_str(bash).unwrap();
    assert!(check(&wf, &ConfigLayer::default())
        .iter()
        .any(|e| matches!(e, CheckError::ContextOnUnsupportedNode { .. })));
}

// --- minor check rules ----------------------------------------------------------

#[test]
fn max_parallel_nodes_zero_is_a_check_error() {
    let wf: Workflow =
        serde_norway::from_str("name: x\nnodes:\n  - { id: a, kind: bash, run: \"true\" }\n")
            .unwrap();
    let config: ConfigLayer =
        serde_norway::from_str("defaults: { max_parallel_nodes: 0 }").unwrap();
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
fn a_push_to_the_base_branch_without_a_prior_gate_warns() {
    let config: ConfigLayer = serde_norway::from_str("project: { base_branch: main }").unwrap();

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
    let wf: Workflow = serde_norway::from_str(ungated).unwrap();
    assert!(
        check_warnings(&wf, &config)
            .iter()
            .any(|w| matches!(w, CheckWarning::PushToBaseWithoutGate { node, .. } if node.as_str() == "pr")),
        "got: {:?}",
        check_warnings(&wf, &config)
    );

    // The same push behind a gate is deliberate — no warning: a gate
    // upstream is the carve-out.
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
    let wf: Workflow = serde_norway::from_str(gated).unwrap();
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
    let wf: Workflow = serde_norway::from_str(templated).unwrap();
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
    let wf: Workflow = serde_norway::from_str(run_branch).unwrap();
    assert!(check_warnings(&wf, &config)
        .iter()
        .all(|w| !matches!(w, CheckWarning::PushToBaseWithoutGate { .. })));
}

// --- scope_expansion ceiling in check --------------------------------------------

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
    let wf: Workflow = serde_norway::from_str(workflow).unwrap();
    let ceiling: ConfigLayer =
        serde_norway::from_str("permissions: { scope_expansion: { max_mode: ask } }").unwrap();
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
    let wf_deny: Workflow = serde_norway::from_str(&deny_node).unwrap();
    assert!(!check(&wf_deny, &ceiling)
        .iter()
        .any(|e| matches!(e, CheckError::ScopeExpansionModeOverCeiling { .. })));
    assert!(!check(&wf, &ConfigLayer::default())
        .iter()
        .any(|e| matches!(e, CheckError::ScopeExpansionModeOverCeiling { .. })));
}

// --- resume_session only where a session exists ----------------------------------

#[test]
fn resume_session_on_a_non_prompt_node_is_refused() {
    let yaml = r#"
name: x
nodes:
  - id: build
    kind: bash
    run: "true"
    on_interrupt: resume_session
"#;
    let wf: Workflow = serde_norway::from_str(yaml).unwrap();
    assert!(
        check(&wf, &ConfigLayer::default()).iter().any(
            |e| matches!(e, CheckError::ResumeSessionOnSessionlessNode { node }
                if node.as_str() == "build")
        ),
        "got: {:?}",
        check(&wf, &ConfigLayer::default())
    );

    let prompt = r#"
name: x
nodes:
  - id: work
    kind: prompt
    prompt: "go"
    on_interrupt: resume_session
"#;
    let wf: Workflow = serde_norway::from_str(prompt).unwrap();
    assert!(!check(&wf, &ConfigLayer::default())
        .iter()
        .any(|e| matches!(e, CheckError::ResumeSessionOnSessionlessNode { .. })));
}

// --- mount declaration rules -------------------------------------------------

fn parsed(yaml: &str) -> Workflow {
    serde_norway::from_str(yaml).unwrap()
}

#[test]
fn a_mount_naming_an_unknown_node_is_refused() {
    let wf = parsed(
        r#"
name: parent
nodes:
  - id: cons
    kind: workflow
    use: consumer
    mounts:
      - artifact: { node: ghost, name: report.md }
"#,
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::BrokenReference { node, field, target }
                if node.as_str() == "cons" && field == "mounts" && target.as_str() == "ghost"
        )),
        "got: {errors:?}"
    );
}

#[test]
fn a_mount_on_the_node_itself_is_refused() {
    let wf = parsed(
        r#"
name: parent
nodes:
  - id: cons
    kind: workflow
    use: consumer
    mounts:
      - artifact: { node: cons, name: report.md }
"#,
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::MountOnSelf { node } if node.as_str() == "cons"
        )),
        "got: {errors:?}"
    );
}

#[test]
fn a_mount_targeting_a_fanned_out_node_is_refused() {
    // Once `review` is review@a + review@b there is no "the" sibling to
    // mount from — same reasoning as goto/gate-on onto a fan-out.
    let wf = parsed(
        r#"
name: parent
nodes:
  - id: review
    kind: prompt
    prompt: "review"
    runners: [a, b]
  - id: cons
    kind: workflow
    use: consumer
    mounts:
      - artifact: { node: review, name: findings.yaml }
"#,
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::MountOnFanOut { node, target }
                if node.as_str() == "cons" && target.as_str() == "review"
        )),
        "got: {errors:?}"
    );
}

#[test]
fn a_mount_inside_a_parallel_group_is_refused() {
    // Parallel children run concurrently — there is no DAG order inside
    // the group, so "hermanos terminados" (siblings having finished) cannot hold there.
    let wf = parsed(
        r#"
name: parent
nodes:
  - id: prod
    kind: bash
    run: "true"
  - id: group
    kind: parallel
    nodes:
      - id: cons
        kind: workflow
        use: consumer
        mounts:
          - artifact: { node: prod, name: report.md }
"#,
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::MountInsideParallel { group, node }
                if group.as_str() == "group" && node.as_str() == "cons"
        )),
        "got: {errors:?}"
    );
}

#[test]
fn a_cycle_formed_only_through_a_mount_is_caught() {
    // The mount implies depends_on — check's own expansion must
    // see the edge, or this deadlocks a real run instead of failing
    // statically.
    let wf = parsed(
        r#"
name: parent
nodes:
  - id: a
    kind: workflow
    use: child-a
    mounts:
      - artifact: { node: b, name: out.md }
  - id: b
    kind: bash
    run: "true"
    depends_on: [a]
"#,
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CheckError::DependsOnCycle { .. })),
        "got: {errors:?}"
    );
}

// --- artifact names stay under run.dir/artifacts/ ------------------------------

/// The identity of an interpreted artifact is `(node, kind)`, so there
/// is no second one of a kind for a node to declare.
#[test]
fn a_node_declaring_the_same_kind_twice_is_refused() {
    let yaml = r#"
name: twice
nodes:
  - id: plan
    kind: prompt
    prompt: "plan it"
    artifacts:
      produces: [tasks, tasks]
"#;
    let wf: Workflow = serde_norway::from_str(yaml).expect("the fixture parses");
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::DuplicateArtifactKind { node, kind }
                if node.as_str() == "plan" && *kind == yunta_core::ArtifactKind::Tasks
        )),
        "got: {errors:?}"
    );
    let text = errors[0].to_string();
    assert!(
        text.contains("at most one") && text.contains("identified by its kind"),
        "the refusal names the rule: {text}"
    );
}

/// A document that enters as an input and a node that produces the same
/// kind are two producers of one identity, with nothing to order them.
#[test]
fn an_input_document_and_a_node_producing_its_kind_are_refused_together() {
    let yaml = r#"
name: two-producers
inputs:
  plan:
    type: document
    kind: tasks
nodes:
  - id: plan-it
    kind: prompt
    prompt: "plan it"
    artifacts:
      produces: [tasks]
"#;
    let wf: Workflow = serde_norway::from_str(yaml).expect("the fixture parses");
    let errors = check(&wf, &ConfigLayer::default());
    let clash = errors
        .iter()
        .find(|e| matches!(e, CheckError::InputDocumentAlsoProduced { .. }))
        .unwrap_or_else(|| panic!("got: {errors:?}"));
    let text = clash.to_string();
    assert!(
        text.contains("`plan`") && text.contains("`plan-it`") && text.contains("tasks document"),
        "the refusal names both producers and the document they claim: {text}"
    );
}

/// A document input whose kind no node produces is exactly what the
/// input is for.
#[test]
fn an_input_document_of_a_kind_nobody_produces_is_accepted() {
    let yaml = r#"
name: one-producer
inputs:
  plan:
    type: document
    kind: tasks
nodes:
  - id: work
    kind: loop
    until: all_tasks_complete
    prompt: "do the task"
"#;
    let wf: Workflow = serde_norway::from_str(yaml).expect("the fixture parses");
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CheckError::InputDocumentAlsoProduced { .. })),
        "got: {errors:?}"
    );
}

/// `tasks`, `findings` and `questions` name the documents the engine
/// reads, so none of them is available as a file name.
#[test]
fn a_reference_naming_a_kind_as_a_file_name_is_refused() {
    let yaml = r#"
name: reserved
nodes:
  - id: review
    kind: prompt
    prompt: "review it"
    artifacts:
      produces: [findings]
  - id: fix
    kind: prompt
    prompt: "fix it"
    context:
      - artifact: { node: review, name: findings }
"#;
    let wf: Workflow = serde_norway::from_str(yaml).expect("the fixture parses");
    let errors = check(&wf, &ConfigLayer::default());
    let reserved = errors
        .iter()
        .find(|e| matches!(e, CheckError::ReservedArtifactName { name, .. } if name == "findings"))
        .unwrap_or_else(|| panic!("got: {errors:?}"));
    let text = reserved.to_string();
    assert!(
        text.contains("kind: findings") && text.contains("`fix`"),
        "the refusal names the form that works and where it was written: {text}"
    );
}

#[test]
fn an_artifact_name_the_run_could_not_take_is_refused() {
    for name in [
        "../escape.md",
        "/tmp/escape.md",
        "notes/../../escape.md",
        "tasks.yaml",
        "questions.answers.yaml",
    ] {
        let mut node = bash("a", "true", &[]);
        node.artifacts = Some(yunta_core::Artifacts {
            produces: vec![yunta_core::ArtifactSpec::Opaque(name.to_string())],
        });
        let errors = check(&workflow(vec![node]), &ConfigLayer::default());
        assert!(
            errors.iter().any(|e| matches!(
                e,
                CheckError::ArtifactNameRefused { node, said }
                    if node.as_str() == "a" && said.contains(name)
            )),
            "`{name}` must be refused, got {errors:?}"
        );
    }

    let mut node = bash("a", "true", &[]);
    node.artifacts = Some(yunta_core::Artifacts {
        produces: vec![yunta_core::ArtifactSpec::Opaque(
            "reports/report-{{runner.role}}.md".to_string(),
        )],
    });
    assert!(
        !check(&workflow(vec![node]), &ConfigLayer::default())
            .iter()
            .any(|e| matches!(e, CheckError::ArtifactNameRefused { .. })),
        "a relative name, subdirectory and template included, is fine"
    );
}

#[test]
fn unknown_filter_is_a_check_error() {
    // `run-events: { filter: ... }` is a closed vocabulary. An unknown
    // value is rejected when `yunta check` reads the workflow — naming
    // the bad value — rather than reaching the resolver as a string it
    // must reject at runtime.
    let yaml = r#"
name: bad-filter
nodes:
  - id: read
    kind: prompt
    runner: executor
    prompt: "x"
    context:
      - run-events: { filter: bogus }
"#;
    let err = yunta_core::yaml::parse::<Workflow>(yaml).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("bogus") || msg.contains("failed"),
        "the rejection names the bad filter or the valid vocabulary: {msg}"
    );
}

// --- any node may declare an interpreted artifact -----------------------------

/// A workflow whose single node `plan` produces a tasks document, with the
/// node's own kind lines spliced in.
fn producing_tasks(node_kind: &str) -> Workflow {
    let yaml = format!(
        r#"
name: tasks
nodes:
  - id: plan
{node_kind}
    artifacts:
      produces: [tasks]
"#
    );
    serde_norway::from_str(&yaml).expect("the fixture parses")
}

#[test]
fn every_node_kind_may_declare_an_interpreted_artifact() {
    // A session hands its document to the run tools; a command writes
    // the file, as `run-tasks` does when it stages a tasks document a person
    // wrote. Both end at the same close, reading the same file through
    // the same door, so neither is a kind of node the declaration is
    // wrong on.
    for node_kind in [
        "    kind: prompt\n    prompt: \"plan it\"",
        "    kind: workflow\n    use: planner",
        "    kind: bash\n    run: \"cp tasks.yaml {{node.artifacts}}/tasks.yaml\"",
    ] {
        assert_eq!(
            check(&producing_tasks(node_kind), &ConfigLayer::default()),
            Vec::new(),
            "`{node_kind}` may declare a tasks document"
        );
    }
}

// --- what a workflow asks of its adapters ------------------------------

/// A capability set with one flag on and everything else off.
fn only(capability: yunta_core::Capability) -> yunta_core::Capabilities {
    let mut declared = yunta_core::Capabilities::default();
    match capability {
        yunta_core::Capability::PermissionProfiles => declared.permission_profiles = true,
        yunta_core::Capability::CustomAgents => declared.custom_agents = true,
        yunta_core::Capability::ResumeSession => declared.resume_session = true,
        yunta_core::Capability::Fence => declared.fence = yunta_core::FenceLevel::ToolCalls,
        yunta_core::Capability::UsageReporting => declared.usage_reporting = true,
        yunta_core::Capability::Skills => declared.skills = true,
        yunta_core::Capability::RunTools => declared.run_tools = true,
        yunta_core::Capability::NetworkIsolation => declared.network_isolation = true,
    }
    declared
}

/// A `bash`-free node on runner `planner`, declaring `permissions:` or
/// `agent:` as the caller asks.
fn asking_node(yaml: &str) -> Workflow {
    workflow(vec![serde_norway::from_str(yaml).expect("the node parses")])
}

#[test]
fn check_refuses_read_only_on_an_adapter_without_permission_profiles() {
    let wf = asking_node(
        "{ id: audit, kind: prompt, runner: planner, prompt: \"look\", permissions: read-only }",
    );
    let config = config_with_runner("planner", 1);

    let errors = check_against(&wf, &config, &|_| Some(yunta_core::Capabilities::default()));
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::CapabilityUnsupported { capability, .. }
                if *capability == yunta_core::Capability::PermissionProfiles
        )),
        "a profile the adapter cannot distinguish is refused before a run: {errors:?}"
    );

    let allowed = check_against(&wf, &config, &|_| {
        Some(only(yunta_core::Capability::PermissionProfiles))
    });
    assert!(
        allowed.is_empty(),
        "an adapter that has it runs the same workflow: {allowed:?}"
    );
}

#[test]
fn check_refuses_an_agent_on_an_adapter_without_custom_agents() {
    let wf = asking_node(
        "{ id: audit, kind: prompt, runner: planner, prompt: \"look\", agent: reviewer }",
    );
    let config = config_with_runner("planner", 1);

    let errors = check_against(&wf, &config, &|_| Some(yunta_core::Capabilities::default()));
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::CapabilityUnsupported { capability, .. }
                if *capability == yunta_core::Capability::CustomAgents
        )),
        "an agent the adapter cannot select is refused before a run: {errors:?}"
    );

    let allowed = check_against(&wf, &config, &|_| {
        Some(only(yunta_core::Capability::CustomAgents))
    });
    assert!(allowed.is_empty(), "{allowed:?}");
}

/// A binary that does not build the adapter judges nothing: a capability
/// it cannot see is not one it can call absent.
#[test]
fn an_adapter_this_binary_does_not_build_refuses_nothing() {
    let wf = asking_node("{ id: audit, kind: prompt, runner: planner, prompt: \"look\", permissions: read-only, agent: reviewer }");
    let errors = check_against(&wf, &config_with_runner("planner", 1), &|_| None);
    assert!(errors.is_empty(), "{errors:?}");
}

/// A runner that fans out is refused only when not one of its adapters
/// can do what the node asks: the run takes the first available
/// candidate, so one that cannot is not a workflow that cannot run.
#[test]
fn a_fan_out_is_refused_only_when_no_candidate_can_do_it() {
    let wf = asking_node(
        "{ id: audit, kind: prompt, runner: planner, prompt: \"look\", permissions: read-only }",
    );
    let config = ConfigLayer {
        runners: Some(BTreeMap::from([(
            "planner".into(),
            vec![
                RunnerCandidate {
                    adapter: "plain".into(),
                    model: "m".into(),
                    agent: None,
                },
                RunnerCandidate {
                    adapter: "fancy".into(),
                    model: "m".into(),
                    agent: None,
                },
            ],
        )])),
        ..Default::default()
    };

    let one_can = check_against(&wf, &config, &|adapter| {
        Some(if adapter.as_str() == "fancy" {
            only(yunta_core::Capability::PermissionProfiles)
        } else {
            yunta_core::Capabilities::default()
        })
    });
    assert!(
        one_can.is_empty(),
        "one candidate can, so the run can: {one_can:?}"
    );

    let neither_can = check_against(&wf, &config, &|_| Some(yunta_core::Capabilities::default()));
    assert_eq!(neither_can.len(), 1, "{neither_can:?}");
}
