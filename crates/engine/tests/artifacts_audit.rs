//! One node's artifacts are out of every other node's reach.
//!
//! A session writes files under the run's `artifacts/` and a CLI grants
//! writes by directory rather than by file, so nothing stops a node from
//! writing under a name another node produces. It reaches nothing by
//! doing so: an artifact is what the run accepted — an identity, a
//! producer and a hash on the log, with the bytes in the run's object
//! store — and every reader resolves it there. The directory is the view
//! the run writes from what it holds.

use yunta_core::events::ArtifactId;
use yunta_engine::RunTerminal;
use yunta_testkit::Bench;

/// `alpha` produces `report.md`; `beta` declares `notes.md` of its own and
/// writes `report.md` besides; `reader` asks for `alpha`'s.
const THREE_NODES: &str = r#"
name: out-of-reach
nodes:
  - id: alpha
    kind: prompt
    runner: executor
    prompt: "Write the report."
    artifacts:
      produces: [report.md]
  - id: beta
    kind: prompt
    runner: executor
    depends_on: [alpha]
    prompt: "Write your notes."
    artifacts:
      produces: [notes.md]
  - id: reader
    kind: prompt
    runner: executor
    depends_on: [beta]
    prompt: "Read the report."
    context:
      - artifact: { node: alpha, name: report.md }
"#;

#[tokio::test]
async fn a_node_that_writes_over_another_nodes_name_reaches_nothing() {
    let bench = Bench::new();
    let dir = bench.run_dir().join("artifacts");
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{dir}/report.md", content: "ALPHA-REPORT" }}
    outcome: {{ type: completed, summary: reported }}
  - effects:
      - {{ path: "{dir}/notes.md", content: "BETA-NOTES" }}
      - {{ path: "{dir}/report.md", content: "BETA-OVERWRITE" }}
    outcome: {{ type: completed, summary: noted }}
  - match_prompt_contains: "ALPHA-REPORT"
    outcome: {{ type: completed, summary: read }}
"#,
        dir = dir.display()
    );

    let (terminal, state) = bench.run(THREE_NODES, &fixture).await;
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "`reader` is served `alpha`'s report, whatever `beta` wrote: {state:?}"
    );

    // The run holds one artifact per producer, and `beta`'s write under
    // `alpha`'s name is none of them: no acceptance accounts for it.
    let held = bench.accepted();
    assert_eq!(
        held.iter()
            .map(|a| (
                a.producer.as_ref().map(|n| n.to_string()),
                a.artifact.to_string()
            ))
            .collect::<Vec<_>>(),
        vec![
            (Some("alpha".to_string()), "report.md".to_string()),
            (Some("beta".to_string()), "notes.md".to_string()),
        ],
        "{held:?}"
    );
    assert_eq!(
        bench.object(&held[0].content_hash).expect("the object"),
        b"ALPHA-REPORT",
        "the bytes `alpha` produced are the bytes the run keeps for it"
    );
    assert_eq!(
        bench
            .projection(Some("alpha"), "report.md")
            .expect("alpha's view"),
        b"ALPHA-REPORT",
        "the view under the producer is written from what the run holds"
    );
}

#[tokio::test]
async fn two_producers_of_one_name_hold_two_artifacts() {
    let bench = Bench::new();
    let dir = bench.run_dir().join("artifacts");
    let workflow = r#"
name: one-name-two-producers
nodes:
  - id: alpha
    kind: prompt
    runner: executor
    prompt: "Write the report."
    artifacts:
      produces: [report.md]
  - id: beta
    kind: prompt
    runner: executor
    depends_on: [alpha]
    prompt: "Write the report."
    artifacts:
      produces: [report.md]
  - id: reader
    kind: prompt
    runner: executor
    depends_on: [beta]
    prompt: "Read the first report."
    context:
      - artifact: { node: alpha, name: report.md }
"#;
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{dir}/report.md", content: "ALPHA-REPORT" }}
    outcome: {{ type: completed, summary: reported }}
  - effects:
      - {{ path: "{dir}/report.md", content: "BETA-REPORT" }}
    outcome: {{ type: completed, summary: reported }}
  - match_prompt_contains: "ALPHA-REPORT"
    outcome: {{ type: completed, summary: read }}
"#,
        dir = dir.display()
    );

    let (terminal, state) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    let held = bench.accepted();
    assert_eq!(held.len(), 2, "one identity per producer: {held:?}");
    assert!(held.iter().all(|a| a.artifact
        == ArtifactId::Opaque {
            name: "report.md".to_string()
        }));
    assert_ne!(
        held[0].content_hash, held[1].content_hash,
        "each producer's own bytes, side by side"
    );
    assert_eq!(
        bench
            .projection(Some("beta"), "report.md")
            .expect("beta's view"),
        b"BETA-REPORT",
        "each producer's view sits under its own node"
    );
}
