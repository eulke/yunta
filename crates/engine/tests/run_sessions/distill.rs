//! A distill that names an artifact no node produced: the path becomes a finding,
//! the rest of the distill lands, and the provenance marks what is missing.

use super::*;

#[tokio::test]
async fn a_distill_path_never_produced_becomes_a_finding_and_the_rest_lands() {
    // Run in `alpha` mode: `notes` never runs, its artifact never
    // exists, but distill declares it.
    let bench = Bench::new().in_mode("alpha");
    let workflow = distiller_without_notes();
    let fixture = format!(
        r#"
dialect: claude-code
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.md", content: "plan\n" }}
    outcome: {{ type: completed, summary: "planned" }}
"#,
        artifacts = bench.staging("plan").display()
    );
    let RunReport { terminal, .. } = bench.run(&workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let dest = bench
        .worktree
        .join(".yunta/knowledge/distilled/distiller")
        .join(bench.run_id.as_str());
    assert!(dest.join("plan.md").exists(), "the produced path lands");
    assert!(!dest.join("notes.md").exists());
    assert_missing_path_is_a_finding(&bench.events());
    assert_provenance_marks_the_missing_path(&dest);
}

/// The distiller, with `notes` outside the `alpha` mode the test runs in: its
/// artifact never exists, but `distill` declares it.
fn distiller_without_notes() -> String {
    let workflow = r#"
name: distiller
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the plan to {{node.artifacts}}/plan.md."
    artifacts:
      produces: [plan.md]
  - id: notes
    kind: prompt
    runner: executor
    depends_on: [plan]
    prompt: "Maybe write notes."
    artifacts:
      produces: [notes.md]
on_finish:
  - distill: [{ node: plan, name: plan.md }, { node: notes, name: notes.md }]
"#;
    // `notes` fails before producing its artifact — but with a re-route
    // budget of zero the run pauses... instead: notes produces, then we
    // delete it? Simpler: notes' fixture writes the artifact and the run
    // finishes, then this test only covers the produced path. The
    // missing-path case uses a workflow whose declared artifact the
    // session legitimately produced but distill names one more — which
    // check would refuse. So: simulate runtime-missing by removing the
    // file after the run? No — distill runs inside execute_run. The
    // honest runtime-missing case: `notes` is mode-excluded.
    workflow.replace(
        "on_finish:",
        "modes:\n  alpha: { include: [plan] }\n  beta: { include: all }\non_finish:",
    )
}

/// The path nothing produced is a finding the engine reported: minor, and it
/// names the missing artifact.
fn assert_missing_path_is_a_finding(events: &[yunta_core::events::StoredEvent]) {
    let finding = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Findings(FindingEvent::Posted(p))) => {
                Some(&p.finding)
            }
            _ => None,
        })
        .expect("the missing path must become a finding, never be lost");
    assert!(
        matches!(
            &finding.says,
            yunta_core::events::FindingSays::Found {
                cause: yunta_core::events::FindingCause::DistillMissing { artifact, .. },
            } if artifact.to_string().contains("notes.md")
        ),
        "got: {finding:?}"
    );
    assert_eq!(finding.severity, yunta_core::events::FindingSeverity::Minor);
}

/// The provenance lists both artifacts in the order distill names them, and
/// marks the one nothing produced as missing.
fn assert_provenance_marks_the_missing_path(dest: &std::path::Path) {
    let provenance: yunta_core::yaml::Value =
        yunta_core::yaml::parse(&std::fs::read_to_string(dest.join("provenance.yaml")).unwrap())
            .unwrap();
    let listed: Vec<&str> = provenance["artifacts"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(listed, vec!["plan.md", "notes.md"]);
    assert_eq!(provenance["artifacts"][1]["missing"].as_bool(), Some(true));
}
