#[test]
fn exposes_crate_identity() {
    assert_eq!(yunta_core::CRATE_NAME, "yunta-core");
}

// --- DI-13: the reference YAMLs are real fixtures (T1.1/T1.2's ✓) ------------

#[test]
fn the_reference_config_parses_and_round_trips() {
    let yaml = include_str!("fixtures/reference-config.yaml");
    let layer: yunta_core::ConfigLayer =
        serde_yaml::from_str(yaml).expect("the reference config must parse whole");

    assert_eq!(layer.version, Some(1));
    assert_eq!(
        layer.defaults.as_ref().unwrap().runner.as_deref(),
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
    assert!(!layer.telemetry.as_ref().unwrap().enabled);
    assert_eq!(
        layer.adapters.as_ref().unwrap()["codex"]
            .adapter_settings
            .as_ref()
            .unwrap()["sandbox"],
        serde_json::Value::String("workspace-write".to_string())
    );

    // Round-trip at the serde-tree level: what parses serializes back to
    // the same value.
    let reserialized = serde_yaml::to_string(&layer).unwrap();
    let reparsed: yunta_core::ConfigLayer = serde_yaml::from_str(&reserialized).unwrap();
    assert_eq!(layer, reparsed);
}

#[test]
fn the_reference_workflow_parses_and_round_trips() {
    let yaml = include_str!("fixtures/build-feature.yaml");
    let workflow: yunta_core::Workflow =
        serde_yaml::from_str(yaml).expect("build-feature.yaml must parse whole");

    assert_eq!(workflow.yunta_schema.as_deref(), Some(">=1 <2"));
    assert_eq!(workflow.nodes.len(), 11);
    assert_eq!(workflow.on_finish.len(), 2);
    let grill = &workflow.nodes[0];
    assert_eq!(grill.skills, vec!["grill"]);
    assert_eq!(grill.interactive, Some(true));
    let implement = &workflow.nodes[3];
    assert_eq!(implement.fresh_context, Some(true));
    assert!(implement.invariant);
    let review = workflow
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "review")
        .unwrap();
    assert_eq!(review.runners, vec!["reviewer", "reviewer-alt"]);

    let reserialized = serde_yaml::to_string(&workflow).unwrap();
    let reparsed: yunta_core::Workflow = serde_yaml::from_str(&reserialized).unwrap();
    assert_eq!(workflow, reparsed);
}
