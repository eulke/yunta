use yunta_core::{
    ArtifactKind, ArtifactSpec, CheckBuiltin, HookFailurePolicy, JoinPolicy, NodeKind,
    NodePermissions, OnInterrupt, PromptSource, Workflow,
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
        NodeKind::Loop { until, prompt, .. } => {
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
fn modes_include_all_round_trips_through_serialization() {
    // Regression: the untagged enum's derived `Serialize` would emit the
    // unit variant `All` as YAML `null`, not the string `"all"` the
    // schema's own `include: all` reads — breaking every
    // manifest round-trip (`status`/`resume` reading back what `run`
    // just wrote) for any workflow using it. Caught by hand against the
    // real binary, not by this suite the first time around — this test
    // is what should have caught it.
    let yaml = r#"
name: fixture
modes:
  full: { include: all }
nodes:
  - id: a
    kind: bash
    run: "true"
"#;
    let first: Workflow = serde_yaml::from_str(yaml).unwrap();
    let re_serialized = serde_yaml::to_string(&first).unwrap();
    let second: Workflow =
        serde_yaml::from_str(&re_serialized).expect("include: all must round-trip");
    assert_eq!(first, second);
    assert_eq!(
        first.modes.as_ref().unwrap()["full"].include,
        yunta_core::ModeInclude::All
    );
}

#[test]
fn modes_include_a_node_list_round_trips_and_declaration_order_survives() {
    let yaml = r#"
name: fixture
modes:
  hotfix:   { include: [a, c] }
  standard: { include: [a, b, c] }
nodes:
  - id: a
    kind: bash
    invariant: true
    run: "true"
  - id: b
    kind: bash
    run: "true"
  - id: c
    kind: bash
    run: "true"
"#;
    let first: Workflow = serde_yaml::from_str(yaml).unwrap();
    let re_serialized = serde_yaml::to_string(&first).unwrap();
    let second: Workflow = serde_yaml::from_str(&re_serialized).unwrap();
    assert_eq!(first, second);

    // The promotion-ladder guarantee: declaration order, not
    // alphabetical or any other reordering.
    let names: Vec<&str> = first
        .modes
        .as_ref()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(names, vec!["hotfix", "standard"]);
    assert!(first.nodes[0].invariant);
    assert!(!first.nodes[1].invariant);
}

#[test]
fn an_internal_gate_parses_the_reference_approve_plan_shape_round_trip() {
    // The exact fragment `build-feature.yaml` (Config y
    // workflows de referencia) declares; the reference YAML doubles as
    // the parse fixture.
    let yaml = r#"
name: fixture
nodes:
  - id: plan
    kind: bash
    run: "true"
  - id: approve-plan
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Plan registrado. Do you approve?"
    options: [aprobar, ajustar, abortar]
    on: { ajustar: plan }
"#;
    let first: Workflow = serde_yaml::from_str(yaml).unwrap();
    let NodeKind::Gate {
        assignee,
        message,
        options,
        on,
        external,
    } = &first.nodes[1].kind
    else {
        panic!("expected a gate");
    };
    assert_eq!(assignee, "lead");
    assert_eq!(message.as_deref(), Some("Plan registrado. Do you approve?"));
    assert_eq!(options, &["aprobar", "ajustar", "abortar"]);
    assert_eq!(on.get("ajustar").map(|t| t.as_str()), Some("plan"));
    assert!(external.is_none(), "an internal gate has no external block");

    let re_serialized = serde_yaml::to_string(&first).unwrap();
    let second: Workflow = serde_yaml::from_str(&re_serialized).unwrap();
    assert_eq!(first, second);
}

#[test]
fn invariant_defaults_to_false() {
    let yaml = r#"
name: fixture
nodes:
  - id: a
    kind: bash
    run: "true"
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert!(!workflow.nodes[0].invariant);
}

#[test]
fn a_workflow_with_no_modes_at_all_parses_with_none() {
    let yaml = r#"
name: fixture
nodes:
  - id: a
    kind: bash
    run: "true"
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert!(workflow.modes.is_none());
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
        NodeKind::Parallel { join, nodes, .. } => {
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
fn a_check_node_parses_baseline_compare_with_no_extra_fields() {
    let yaml = r#"
id: no-regressions
kind: check
builtin: baseline_compare
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Check(builtin) => assert_eq!(builtin, CheckBuiltin::BaselineCompare),
        other => panic!("expected Check, got {other:?}"),
    }
}

#[test]
fn a_check_node_parses_coverage_gate() {
    let yaml = r#"
id: coverage
kind: check
builtin: coverage_gate
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Check(builtin) => assert_eq!(builtin, CheckBuiltin::CoverageGate),
        other => panic!("expected Check, got {other:?}"),
    }
}

#[test]
fn a_check_node_parses_findings_gate_with_its_max_severity() {
    let yaml = r#"
id: no-blocking-findings
kind: check
builtin: findings_gate
max_severity: major
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Check(builtin) => assert_eq!(
            builtin,
            CheckBuiltin::FindingsGate {
                max_severity: yunta_core::events::FindingSeverity::Major,
            }
        ),
        other => panic!("expected Check, got {other:?}"),
    }
}

#[test]
fn an_unknown_check_builtin_fails_to_parse() {
    let yaml = r#"
id: mystery
kind: check
builtin: something_undefined
"#;
    let result: Result<yunta_core::Node, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "unknown builtin must not parse");
}

#[test]
fn an_executor_node_parses_with_default_empty_with_and_no_timeout() {
    let yaml = r#"
id: coverage-gate
kind: executor
executor: coverage-gate
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Executor {
            executor,
            with,
            timeout_seconds,
        } => {
            assert_eq!(executor, "coverage-gate");
            assert!(with.is_empty());
            assert_eq!(timeout_seconds, None);
        }
        other => panic!("expected Executor, got {other:?}"),
    }
}

#[test]
fn an_executor_node_parses_with_params_and_a_timeout() {
    let yaml = r#"
id: coverage-gate
kind: executor
executor: coverage-gate
with:
  threshold: 80
  suite: unit
timeout_seconds: 30
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Executor {
            executor,
            with,
            timeout_seconds,
        } => {
            assert_eq!(executor, "coverage-gate");
            assert_eq!(with.get("threshold").unwrap(), &serde_json::json!(80));
            assert_eq!(with.get("suite").unwrap(), &serde_json::json!("unit"));
            assert_eq!(timeout_seconds, Some(30));
        }
        other => panic!("expected Executor, got {other:?}"),
    }
}

#[test]
fn a_loop_node_s_scope_expansion_defaults_to_absent() {
    let yaml = r#"
id: implement
kind: loop
until: all_tasks_complete
prompt: "do it"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Loop {
            scope_expansion, ..
        } => assert!(scope_expansion.is_none()),
        other => panic!("expected Loop, got {other:?}"),
    }
}

#[test]
fn a_loop_node_can_declare_scope_expansion_ask_with_within_and_cap() {
    let yaml = r#"
id: implement
kind: loop
until: all_tasks_complete
prompt: "do it"
scope_expansion:
  mode: ask
  within: ["src/**"]
  max_per_run: 3
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Loop {
            scope_expansion, ..
        } => {
            let se = scope_expansion.unwrap();
            assert_eq!(se.mode, yunta_core::events::ScopeExpansionMode::Ask);
            assert_eq!(se.within, vec!["src/**".to_string()]);
            assert_eq!(se.max_per_run, Some(3));
        }
        other => panic!("expected Loop, got {other:?}"),
    }
}

#[test]
fn scope_expansion_with_no_mode_defaults_to_deny() {
    let yaml = r#"
id: implement
kind: loop
until: all_tasks_complete
prompt: "do it"
scope_expansion:
  within: ["src/**"]
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Loop {
            scope_expansion, ..
        } => {
            assert_eq!(
                scope_expansion.unwrap().mode,
                yunta_core::events::ScopeExpansionMode::Deny
            );
        }
        other => panic!("expected Loop, got {other:?}"),
    }
}

#[test]
fn a_loop_node_s_concurrency_defaults_to_absent() {
    let yaml = r#"
id: implement
kind: loop
until: all_tasks_complete
prompt: "do it"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Loop { concurrency, .. } => assert_eq!(concurrency, None),
        other => panic!("expected Loop, got {other:?}"),
    }
}

#[test]
fn a_loop_node_can_declare_concurrency() {
    let yaml = r#"
id: implement
kind: loop
until: all_tasks_complete
prompt: "do it"
concurrency: 4
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match node.kind {
        NodeKind::Loop { concurrency, .. } => assert_eq!(concurrency, Some(4)),
        other => panic!("expected Loop, got {other:?}"),
    }
}

#[test]
fn a_node_can_declare_a_permissions_profile() {
    let yaml = r#"
id: plan
kind: prompt
prompt: "plan it"
permissions: read-only
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(node.permissions, Some(NodePermissions::ReadOnly));
}

#[test]
fn a_node_s_permissions_profile_defaults_to_absent() {
    let yaml = r#"
id: implement
kind: bash
run: "true"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(node.permissions, None);
    assert_eq!(node.network, None);
}

#[test]
fn a_node_can_declare_network_false_as_a_pure_declaration() {
    let yaml = r#"
id: offline-lint
kind: bash
run: "cargo clippy"
network: false
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(node.network, Some(false));
}

#[test]
fn an_unknown_permissions_profile_fails_to_parse() {
    let yaml = r#"
id: plan
kind: prompt
prompt: "plan it"
permissions: unrestricted
"#;
    let result: Result<yunta_core::Node, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "unknown profile must not parse");
}

#[test]
fn a_node_s_description_defaults_to_absent() {
    let yaml = r#"
id: implement
kind: bash
run: "true"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(node.description, None);
}

#[test]
fn a_node_can_declare_a_one_line_description_for_progress_md() {
    let yaml = r#"
id: implement
kind: bash
run: "true"
description: "Wires up the CLI's graph command"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        node.description.as_deref(),
        Some("Wires up the CLI's graph command")
    );
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

// --- context: -----------------------------------------------------------

#[test]
fn a_node_s_context_defaults_to_empty() {
    let yaml = r#"
id: plan
kind: prompt
prompt: "plan it"
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    assert!(node.context.is_empty());
}

#[test]
fn every_context_builtin_parses_from_its_own_contrato_example() {
    let yaml = r#"
id: plan
kind: prompt
prompt: "plan it"
context:
  - files: ["docs/architecture.md", "{{run.dir}}/artifacts/brief.md"]
  - command: "git log --oneline -20"
  - artifact: { node: grill, name: brief.md }
  - ledger: {}
  - knowledge: {}
  - node-output: { node: lint }
  - run-events: { filter: failed }
  - mcp: { server: internal-docs, query: "{{inputs.idea}}" }
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(node.context.len(), 8);

    use yunta_core::ContextSpec;
    match &node.context[0] {
        ContextSpec::Files { files } => assert_eq!(
            files,
            &vec![
                "docs/architecture.md".to_string(),
                "{{run.dir}}/artifacts/brief.md".to_string()
            ]
        ),
        other => panic!("expected Files, got {other:?}"),
    }
    match &node.context[1] {
        ContextSpec::Command { command } => assert_eq!(command, "git log --oneline -20"),
        other => panic!("expected Command, got {other:?}"),
    }
    match &node.context[2] {
        ContextSpec::Artifact { artifact } => {
            assert_eq!(artifact.node.as_ref().unwrap().as_str(), "grill");
            assert_eq!(artifact.name, "brief.md");
        }
        other => panic!("expected Artifact, got {other:?}"),
    }
    assert!(matches!(&node.context[3], ContextSpec::Ledger { .. }));
    match &node.context[4] {
        ContextSpec::Knowledge { knowledge } => assert!(knowledge.layers.is_empty()),
        other => panic!("expected Knowledge, got {other:?}"),
    }
    match &node.context[5] {
        ContextSpec::NodeOutput { node_output } => {
            assert_eq!(node_output.node.as_str(), "lint")
        }
        other => panic!("expected NodeOutput, got {other:?}"),
    }
    match &node.context[6] {
        ContextSpec::RunEvents { run_events } => {
            assert_eq!(run_events.filter.as_deref(), Some("failed"))
        }
        other => panic!("expected RunEvents, got {other:?}"),
    }
    match &node.context[7] {
        ContextSpec::Mcp { mcp } => {
            assert_eq!(mcp.server, "internal-docs");
            assert_eq!(mcp.query, "{{inputs.idea}}");
        }
        other => panic!("expected Mcp, got {other:?}"),
    }
}

#[test]
fn a_knowledge_source_can_declare_specific_layers() {
    let yaml = r#"
id: plan
kind: prompt
prompt: "plan it"
context:
  - knowledge: { layers: [repo] }
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match &node.context[0] {
        yunta_core::ContextSpec::Knowledge { knowledge } => {
            assert_eq!(knowledge.layers, vec![yunta_core::KnowledgeLayer::Repo])
        }
        other => panic!("expected Knowledge, got {other:?}"),
    }
}

#[test]
fn a_knowledge_source_can_declare_every_layer_by_name() {
    let yaml = r#"
id: plan
kind: prompt
prompt: "plan it"
context:
  - knowledge: { layers: [repo, user, org] }
"#;
    let node: yunta_core::Node = serde_yaml::from_str(yaml).unwrap();
    match &node.context[0] {
        yunta_core::ContextSpec::Knowledge { knowledge } => assert_eq!(
            knowledge.layers,
            vec![
                yunta_core::KnowledgeLayer::Repo,
                yunta_core::KnowledgeLayer::User,
                yunta_core::KnowledgeLayer::Org,
            ]
        ),
        other => panic!("expected Knowledge, got {other:?}"),
    }
}

#[test]
fn an_unknown_knowledge_layer_name_is_a_parse_error_not_a_runtime_surprise() {
    let yaml = r#"
id: plan
kind: prompt
prompt: "plan it"
context:
  - knowledge: { layers: [galaxy] }
"#;
    // A parse-time error — never something `resolve_knowledge` discovers
    // at run time — that names the field, the value and the layers that
    // exist.
    let err = yunta_core::yaml::parse::<yunta_core::Node>(yaml).unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("layers") && text.contains("`galaxy`") && text.contains("`repo`"),
        "error was: {text}"
    );
}

// --- inputs: --------------------------------------------------------

#[test]
fn every_input_type_parses_with_its_own_type_specific_fields() {
    let yaml = r#"
name: with-inputs
inputs:
  idea:
    type: string
    required: true
    description: "What to build"
  target_branch:
    type: string
    default: "main"
    pattern: "^[a-zA-Z0-9._/-]+$"
  severity_floor:
    type: enum
    values: [blocking, major, minor]
    default: major
  max_tasks:
    type: number
    min: 1
    max: 200
    default: 40
  changelog:
    type: path
  dry_run:
    type: boolean
    default: false
nodes:
  - id: plan
    kind: bash
    run: "true"
"#;
    let workflow: yunta_core::Workflow = serde_yaml::from_str(yaml).expect("should parse");
    assert_eq!(workflow.inputs.len(), 6);

    match &workflow.inputs["idea"] {
        yunta_core::InputSpec::String {
            required,
            default,
            description,
            ..
        } => {
            assert_eq!(*required, Some(true));
            assert_eq!(*default, None);
            assert_eq!(description.as_deref(), Some("What to build"));
        }
        other => panic!("expected String, got {other:?}"),
    }
    match &workflow.inputs["target_branch"] {
        yunta_core::InputSpec::String {
            default, pattern, ..
        } => {
            assert_eq!(default.as_deref(), Some("main"));
            assert_eq!(pattern.as_deref(), Some("^[a-zA-Z0-9._/-]+$"));
        }
        other => panic!("expected String, got {other:?}"),
    }
    match &workflow.inputs["severity_floor"] {
        yunta_core::InputSpec::Enum {
            values, default, ..
        } => {
            assert_eq!(values, &vec!["blocking", "major", "minor"]);
            assert_eq!(default.as_deref(), Some("major"));
        }
        other => panic!("expected Enum, got {other:?}"),
    }
    match &workflow.inputs["max_tasks"] {
        yunta_core::InputSpec::Number {
            min, max, default, ..
        } => {
            assert_eq!(*min, Some(1.0));
            assert_eq!(*max, Some(200.0));
            assert_eq!(*default, Some(40.0));
        }
        other => panic!("expected Number, got {other:?}"),
    }
    assert!(matches!(
        workflow.inputs["changelog"],
        yunta_core::InputSpec::Path { .. }
    ));
    match &workflow.inputs["dry_run"] {
        yunta_core::InputSpec::Boolean { default, .. } => assert_eq!(*default, Some(false)),
        other => panic!("expected Boolean, got {other:?}"),
    }
}

#[test]
fn inputs_round_trip_through_serialization() {
    let yaml = r#"
name: with-inputs
inputs:
  idea:
    type: string
    required: true
nodes:
  - id: plan
    kind: bash
    run: "true"
"#;
    let first: yunta_core::Workflow = serde_yaml::from_str(yaml).unwrap();
    let re_serialized = serde_yaml::to_string(&first).unwrap();
    let second: yunta_core::Workflow = serde_yaml::from_str(&re_serialized).unwrap();
    assert_eq!(first, second);
}

#[test]
fn a_workflow_with_no_inputs_declares_an_empty_map_never_a_missing_field_error() {
    let yaml = r#"
name: no-inputs
nodes:
  - id: plan
    kind: bash
    run: "true"
"#;
    let workflow: yunta_core::Workflow = serde_yaml::from_str(yaml).unwrap();
    assert!(workflow.inputs.is_empty());
}

// --- Reference-schema fields (interactive, yunta_schema, skills,
// on_finish) ------------------------------------------------------------------

#[test]
fn the_reference_workflow_header_fields_round_trip() {
    let yaml = r#"
name: build-feature
yunta_schema: ">=1 <2"
nodes:
  - id: grill
    kind: prompt
    runner: planner
    skills: [grill]
    interactive: true
    prompt: "Ask the questions."
  - id: implement
    kind: loop
    runner: executor
    depends_on: [grill]
    until: all_tasks_complete
    prompt: "Implement."
on_finish:
  - cleanup: worktree
  - distill: [plan.yaml]
"#;
    let wf: yunta_core::Workflow = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(wf.yunta_schema.as_deref(), Some(">=1 <2"));
    assert_eq!(wf.nodes[0].skills, vec!["grill"]);
    assert_eq!(wf.nodes[0].interactive, Some(true));
    assert_eq!(
        wf.on_finish,
        vec![
            yunta_core::OnFinishStep::Cleanup {
                cleanup: yunta_core::CleanupTarget::Worktree
            },
            yunta_core::OnFinishStep::Distill {
                distill: vec!["plan.yaml".to_string()]
            },
        ]
    );

    // Round-trip: serialize and re-parse to the same tree.
    let reserialized = serde_yaml::to_string(&wf).unwrap();
    let reparsed: yunta_core::Workflow = serde_yaml::from_str(&reserialized).unwrap();
    assert_eq!(wf, reparsed);
}

#[test]
fn node_defaults_can_declare_skills_for_every_node() {
    let yaml = r#"
name: defaults
node_defaults:
  skills: [conventions]
nodes:
  - id: only
    kind: bash
    run: "true"
"#;
    let wf: yunta_core::Workflow = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        wf.node_defaults.unwrap().skills,
        vec!["conventions".to_string()]
    );
}
