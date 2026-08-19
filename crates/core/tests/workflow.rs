use yunta_core::{
    ArtifactKind, ArtifactSpec, HookFailurePolicy, JoinPolicy, NodeKind, OnInterrupt, PromptSource,
    Workflow,
};

const FIXTURE: &str = include_str!("fixtures/m0-workflow.yaml");

#[test]
fn parses_the_m0_schema_recorte_without_loss() {
    let workflow: Workflow = serde_yaml::from_str(FIXTURE).expect("fixture should parse");

    assert_eq!(workflow.name, "fix-lint-loop");
    assert_eq!(workflow.nodes.len(), 3);

    let implement = &workflow.nodes[0];
    assert_eq!(implement.id.as_str(), "implement");
    assert_eq!(implement.runner.as_deref(), Some("executor"));
    match &implement.kind {
        NodeKind::Loop { until, prompt } => {
            assert_eq!(until, "all_tasks_complete");
            assert!(matches!(prompt, PromptSource::Inline(_)));
        }
        other => panic!("expected Loop, got {other:?}"),
    }
    let produces = &implement.artifacts.as_ref().unwrap().produces;
    assert_eq!(
        produces[0],
        ArtifactSpec::Typed {
            name: "ledger.yaml".to_string(),
            kind: ArtifactKind::TaskLedger,
        }
    );

    let lint = &workflow.nodes[1];
    assert_eq!(lint.depends_on[0].as_str(), "implement");
    match &lint.kind {
        NodeKind::Bash { run } => assert_eq!(run, "cargo clippy --workspace -- -D warnings"),
        other => panic!("expected Bash, got {other:?}"),
    }
    let on_failure = lint.on_failure.as_ref().unwrap();
    assert_eq!(on_failure.goto.as_str(), "fix-lint");
    assert_eq!(on_failure.max_reroutes, 2);

    let fix_lint = &workflow.nodes[2];
    assert_eq!(fix_lint.scope, vec!["src/**".to_string()]);
    let hooks = fix_lint.hooks.as_ref().unwrap();
    assert_eq!(hooks.after[0].run, "cargo fmt");
    assert!(hooks.before.is_empty());
}

#[test]
fn round_trips_through_serialization() {
    let first: Workflow = serde_yaml::from_str(FIXTURE).unwrap();
    let re_serialized = serde_yaml::to_string(&first).unwrap();
    let second: Workflow = serde_yaml::from_str(&re_serialized).unwrap();

    assert_eq!(first, second);
}

#[test]
fn a_path_looking_scalar_prompt_is_always_literal_text_never_a_file() {
    let yaml = r#"
id: plan
kind: prompt
prompt: "prompts/plan.md"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Prompt { prompt } => {
            assert_eq!(prompt, PromptSource::Inline("prompts/plan.md".to_string()));
        }
        other => panic!("expected Prompt, got {other:?}"),
    }
}

#[test]
fn a_hook_step_defaults_to_fail_with_no_timeout() {
    let yaml = r#"
id: fix-lint
kind: bash
run: "true"
hooks:
  after:
    - run: "cargo fmt"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    let step = &node.hooks.unwrap().after[0];
    assert_eq!(step.on_failure, HookFailurePolicy::Fail);
    assert_eq!(step.timeout_seconds, None);
}

#[test]
fn a_hook_step_parses_warn_and_a_timeout() {
    let yaml = r#"
id: fix-lint
kind: bash
run: "true"
hooks:
  after:
    - run: "rm -rf .tmp-fixtures"
      on_failure: warn
      timeout_seconds: 5
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    let step = &node.hooks.unwrap().after[0];
    assert_eq!(step.on_failure, HookFailurePolicy::Warn);
    assert_eq!(step.timeout_seconds, Some(5));
}

#[test]
fn node_defaults_hooks_parses_at_the_workflow_level() {
    let yaml = r#"
name: with-node-defaults
node_defaults:
  hooks:
    after:
      - run: "cargo fmt"
nodes:
  - id: only
    kind: bash
    run: "true"
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let defaults = workflow.node_defaults.unwrap();
    assert_eq!(defaults.hooks.unwrap().after[0].run, "cargo fmt");
}

#[test]
fn a_parallel_node_parses_its_children_and_defaults_join_to_all() {
    let yaml = r#"
id: pre-launch
kind: parallel
nodes:
  - id: write-docs
    kind: bash
    run: "echo docs"
  - id: load-test
    kind: bash
    run: "echo load"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Parallel { join, nodes } => {
            assert_eq!(join, JoinPolicy::All);
            assert_eq!(nodes.len(), 2);
            assert_eq!(nodes[0].id.as_str(), "write-docs");
            assert_eq!(nodes[1].id.as_str(), "load-test");
        }
        other => panic!("expected Parallel, got {other:?}"),
    }
}

#[test]
fn a_parallel_node_can_declare_join_any() {
    let yaml = r#"
id: pre-launch
kind: parallel
join: any
nodes:
  - id: a
    kind: bash
    run: "true"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Parallel { join, .. } => assert_eq!(join, JoinPolicy::Any),
        other => panic!("expected Parallel, got {other:?}"),
    }
}

#[test]
fn a_node_s_on_interrupt_defaults_to_absent_not_a_forced_choice() {
    let yaml = r#"
id: implement
kind: bash
run: "true"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(node.on_interrupt, None);
}

#[test]
fn a_node_can_declare_fail_if_uncertain() {
    let yaml = r#"
id: implement
kind: prompt
prompt: "do it"
on_interrupt: fail_if_uncertain
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(node.on_interrupt, Some(OnInterrupt::FailIfUncertain));
}

#[test]
fn explicit_file_mapping_is_a_file_reference() {
    let yaml = r#"
id: plan
kind: prompt
prompt: { file: prompts/plan.md }
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Prompt { prompt } => {
            assert_eq!(
                prompt,
                PromptSource::File(std::path::PathBuf::from("prompts/plan.md"))
            );
        }
        other => panic!("expected Prompt, got {other:?}"),
    }
}
