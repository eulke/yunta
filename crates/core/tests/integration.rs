// --- The reference YAMLs are real fixtures -----------------------------------

#[test]
fn the_reference_config_parses_and_round_trips() {
    let yaml = include_str!("fixtures/reference-config.yaml");
    let layer: yunta_core::ConfigLayer =
        serde_norway::from_str(yaml).expect("the reference config must parse whole");

    assert_eq!(layer.version, Some(1));
    assert_eq!(
        layer
            .defaults
            .as_ref()
            .unwrap()
            .runner
            .as_ref()
            .map(|runner| runner.as_str()),
        Some("executor")
    );
    assert_eq!(layer.defaults.as_ref().unwrap().timeout_minutes, Some(45));
    assert_eq!(
        layer.skills.as_ref().unwrap().paths,
        vec![
            std::path::PathBuf::from(".yunta/skills"),
            std::path::PathBuf::from("~/.yunta/skills"),
        ]
    );
    assert_eq!(layer.skills.as_ref().unwrap().always, vec!["conventions"]);
    assert_eq!(
        layer.pricing.as_ref().unwrap()["claude-opus-4-8"].cost_per_1k_tokens,
        0.015
    );
    assert_eq!(layer.secrets, vec!["GITHUB_TOKEN"]);
    assert_eq!(
        layer.adapters.as_ref().unwrap()["codex"]
            .adapter_settings
            .as_ref()
            .unwrap()["sandbox"],
        serde_json::Value::String("workspace-write".to_string())
    );

    // Round-trip at the serde-tree level: what parses serializes back to
    // the same value.
    let reserialized = serde_norway::to_string(&layer).unwrap();
    let reparsed: yunta_core::ConfigLayer = serde_norway::from_str(&reserialized).unwrap();
    assert_eq!(layer, reparsed);
}

#[test]
fn the_reference_workflow_parses_and_round_trips() {
    let yaml = include_str!("fixtures/build-feature.yaml");
    let workflow: yunta_core::Workflow =
        serde_norway::from_str(yaml).expect("build-feature.yaml must parse whole");

    assert_eq!(workflow.yunta_schema.as_deref(), Some(">=1 <2"));
    assert_eq!(workflow.nodes.len(), 11);
    assert_eq!(workflow.on_finish.len(), 2);
    let grill = &workflow.nodes[0];
    assert_eq!(grill.skills, vec!["grill"]);
    assert!(grill.interactive);
    let implement = &workflow.nodes[3];
    assert!(implement.invariant);
    let review = workflow
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "review")
        .unwrap();
    assert_eq!(review.runners, vec!["reviewer", "reviewer-alt"]);

    let reserialized = serde_norway::to_string(&workflow).unwrap();
    let reparsed: yunta_core::Workflow = serde_norway::from_str(&reserialized).unwrap();
    assert_eq!(workflow, reparsed);
}

#[test]
fn the_composed_reference_workflow_parses_and_round_trips() {
    let yaml = include_str!("fixtures/release-cycle.yaml");
    let workflow: yunta_core::Workflow =
        serde_norway::from_str(yaml).expect("release-cycle.yaml must parse whole");

    assert_eq!(workflow.nodes.len(), 5);
    let design = &workflow.nodes[0];
    let yunta_core::NodeKind::Workflow {
        r#use,
        inputs,
        isolation,
        ..
    } = &design.kind
    else {
        panic!("`design` must be a workflow node, got {:?}", design.kind);
    };
    assert_eq!(r#use, "design-review");
    assert_eq!(
        inputs.get("rfc").map(String::as_str),
        Some("{{inputs.rfc}}")
    );
    assert_eq!(*isolation, yunta_core::WorkflowIsolation::Worktree);

    // `qa` declares no `inputs:` at all — the field is optional.
    let qa = workflow
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "qa")
        .unwrap();
    let yunta_core::NodeKind::Workflow { inputs, .. } = &qa.kind else {
        panic!("`qa` must be a workflow node");
    };
    assert!(inputs.is_empty());

    // Workflow nodes nest inside `parallel` groups (the reference's own
    // `build` group).
    let build = workflow
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "build")
        .unwrap();
    let yunta_core::NodeKind::Parallel { nodes, .. } = &build.kind else {
        panic!("`build` must be a parallel group");
    };
    assert!(nodes
        .iter()
        .all(|child| matches!(child.kind, yunta_core::NodeKind::Workflow { .. })));

    let reserialized = serde_norway::to_string(&workflow).unwrap();
    let reparsed: yunta_core::Workflow = serde_norway::from_str(&reserialized).unwrap();
    assert_eq!(workflow, reparsed);
}

#[test]
fn the_promote_knowledge_reference_workflow_parses_and_round_trips() {
    // "candidatos desde los knowledge/ de los repos → gate
    // con assignee curador → nueva versión del pack" — a plain
    // workflow, no engine mechanism of its own.
    let yaml = include_str!("fixtures/promote-knowledge.yaml");
    let workflow: yunta_core::Workflow =
        serde_norway::from_str(yaml).expect("promote-knowledge.yaml must parse whole");

    assert_eq!(workflow.nodes.len(), 3);
    assert!(workflow.inputs.contains_key("candidates"));
    assert!(workflow.inputs.contains_key("new_version"));

    let gate = workflow
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "approve-promotion")
        .unwrap();
    let yunta_core::NodeKind::Gate {
        assignee, options, ..
    } = &gate.kind
    else {
        panic!("`approve-promotion` must be a gate, got {:?}", gate.kind);
    };
    assert_eq!(assignee, "curator");
    assert_eq!(options, &["approve"]);

    let publish = workflow
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "publish")
        .unwrap();
    assert_eq!(
        publish.depends_on,
        vec![yunta_core::NodeId::from("approve-promotion")]
    );

    let reserialized = serde_norway::to_string(&workflow).unwrap();
    let reparsed: yunta_core::Workflow = serde_norway::from_str(&reserialized).unwrap();
    assert_eq!(workflow, reparsed);
}

#[test]
fn workflow_node_isolation_inherit_parses() {
    let yaml = r#"
name: phased
nodes:
  - id: phase-1
    kind: workflow
    use: implement-phase
    isolation: inherit
    scope: ["src/a/**"]
"#;
    let workflow: yunta_core::Workflow = serde_norway::from_str(yaml).unwrap();
    let yunta_core::NodeKind::Workflow { isolation, .. } = &workflow.nodes[0].kind else {
        panic!("expected a workflow node");
    };
    assert_eq!(*isolation, yunta_core::WorkflowIsolation::Inherit);
}

// --- Cross-run artifact mounts ------------------------------------------------

#[test]
fn workflow_node_mounts_parse_and_round_trip() {
    let yaml = r#"
name: parent
nodes:
  - id: cons
    kind: workflow
    use: consumer
    mounts:
      - artifact: { node: prod, name: report.md }
      - artifact: { node: plan, name: plan.yaml, as: brief.md }
"#;
    let workflow: yunta_core::Workflow = serde_norway::from_str(yaml).unwrap();
    let yunta_core::NodeKind::Workflow { mounts, .. } = &workflow.nodes[0].kind else {
        panic!("expected a workflow node");
    };
    assert_eq!(mounts.len(), 2);
    assert_eq!(mounts[0].artifact.node.as_str(), "prod");
    assert_eq!(mounts[0].artifact.name, "report.md");
    assert!(mounts[0].artifact.rename.is_none());
    assert_eq!(mounts[1].artifact.rename.as_deref(), Some("brief.md"));

    let reserialized = serde_norway::to_string(&workflow).unwrap();
    let reparsed: yunta_core::Workflow = serde_norway::from_str(&reserialized).unwrap();
    assert_eq!(workflow, reparsed);
}

#[test]
fn context_artifact_without_node_parses_and_round_trips() {
    // The node-less form — "an artifact of this run's dir, whoever
    // produced it, a mounted one included" — is what keeps a catalog
    // child parametric: it never has to name a producer it doesn't have.
    let yaml = r#"
name: consumer
nodes:
  - id: talk
    kind: prompt
    prompt: "Use the brief."
    context:
      - artifact: { name: brief.md }
"#;
    let workflow: yunta_core::Workflow = serde_norway::from_str(yaml).unwrap();
    let yunta_core::ContextSpec::Artifact { artifact } = &workflow.nodes[0].context[0] else {
        panic!("expected an artifact context source");
    };
    assert!(artifact.node.is_none());
    assert_eq!(artifact.name, "brief.md");

    let reserialized = serde_norway::to_string(&workflow).unwrap();
    let reparsed: yunta_core::Workflow = serde_norway::from_str(&reserialized).unwrap();
    assert_eq!(workflow, reparsed);
}

// --- `coordination:` on parallel groups ---------------------------------------

#[test]
fn parallel_coordination_parses_defaults_independent_and_round_trips() {
    let yaml = r#"
name: coordinated
nodes:
  - id: cooperative
    kind: parallel
    coordination: blackboard
    nodes:
      - id: a
        kind: bash
        run: "true"
  - id: evaluative
    kind: parallel
    nodes:
      - id: b
        kind: bash
        run: "true"
"#;
    let workflow: yunta_core::Workflow = serde_norway::from_str(yaml).unwrap();
    let yunta_core::NodeKind::Parallel { coordination, .. } = &workflow.nodes[0].kind else {
        panic!("expected a parallel node");
    };
    assert_eq!(*coordination, yunta_core::Coordination::Blackboard);
    let yunta_core::NodeKind::Parallel { coordination, .. } = &workflow.nodes[1].kind else {
        panic!("expected a parallel node");
    };
    // `independent` is the default — evaluative groups (reviewers)
    // must never see each other's findings unless the author opts in.
    assert_eq!(*coordination, yunta_core::Coordination::Independent);

    let reserialized = serde_norway::to_string(&workflow).unwrap();
    let reparsed: yunta_core::Workflow = serde_norway::from_str(&reserialized).unwrap();
    assert_eq!(workflow, reparsed);
}
