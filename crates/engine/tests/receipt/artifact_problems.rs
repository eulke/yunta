//! What a receipt counts of the artifacts a run owed, by their stable codes.

use super::*;

/// A receipt that had to read prose could only reprint it. Counting is
/// what a diagnostic being a value buys: "how often did a document come
/// back unreadable" becomes a number on the receipt instead of a grep
/// over free text.
#[test]
fn the_receipt_counts_artifact_problems_by_their_stable_code() {
    let mut receipt = sample_receipt(EventChainStatus::Intact { events: 342 });
    receipt.diagnostics = vec![
        DiagnosticCount {
            kind: Some(ArtifactKind::Tasks.into()),
            code: DiagnosticCode::Parse(ParseCode::Parse),
            occurrences: 2,
        },
        DiagnosticCount {
            kind: Some(ArtifactKind::Tasks.into()),
            code: DiagnosticCode::Rule(RuleCode::NoCriteria),
            occurrences: 1,
        },
    ];
    let markdown = render_receipt_markdown(&receipt);
    assert!(
        markdown.contains(
            "3 artifact problems reported during the run: `parse` in the tasks document 2 times, \
             `no-criteria` in the tasks document 1 time"
        ),
        "{markdown}"
    );
}

/// A child the workflow names is frozen with the run, so it has to exist for
/// the run to be born; the receipts here read only the failure they name.
fn with_producer_child(bench: &Bench) {
    yunta_testkit::write(
        &bench.worktree.join(".yunta/workflows/producer.yaml"),
        "name: producer\nnodes:\n  - { id: make, kind: bash, run: \"true\" }\n",
    );
}

/// The receipt a run of `workflow_yaml` earns when its one node failed
/// on `failure` and the run ended.
///
/// Derived from a log built here rather than from a run: a node that
/// fails on an artifact leaves its run paused, and a receipt certifies
/// closed work only. What is under test is the derivation from
/// `node_failed`, and that is exactly what the log carries.
async fn receipt_of_failure(workflow_yaml: &str, node: &str, failure: Failure) -> Receipt {
    let bench = Bench::new();
    with_producer_child(&bench);
    let manifest = bench.manifest_for(workflow_yaml, CONFIG).await;

    // A node fails only while it runs, so the log starts it first: a
    // failure with no start before it breaks the log before its close.
    let events = vec![
        StoredEvent {
            run_id: bench.run_id.clone(),
            seq: 1_u64.into(),
            timestamp: yunta_core::Clock::now(&FixedClock),
            node_id: Some(NodeId::from(node)),
            body: EventBody::Known(EventPayload::Node(NodeEvent::Started(
                NodeStartedPayload::attempt(1),
            ))),
        },
        StoredEvent {
            run_id: bench.run_id.clone(),
            seq: 2_u64.into(),
            timestamp: yunta_core::Clock::now(&FixedClock),
            node_id: Some(NodeId::from(node)),
            body: EventBody::Known(EventPayload::Node(NodeEvent::Failed(
                NodeFailedPayload::new(failure, TokenUsage::default()),
            ))),
        },
        StoredEvent {
            run_id: bench.run_id.clone(),
            seq: 3_u64.into(),
            timestamp: yunta_core::Clock::now(&FixedClock),
            node_id: None,
            body: EventBody::Known(EventPayload::Run(RunEvent::Finished(RunFinishedPayload {
                terminal_state: TerminalState::Failed,
                metrics: RunMetrics {
                    cptv: None,
                    tokens: TokenUsage::default(),
                },
                closed_by: None,
            }))),
        },
    ];

    build_receipt(
        &bench.run_id,
        &manifest,
        &events,
        EventChainStatus::Intact {
            events: events.len(),
        },
    )
    .unwrap()
}

/// A `kind: workflow` node declaring what its child run never produced
/// fails on the artifact itself, and the receipt counts that like any
/// other artifact failure — by its stable code, with no document kind,
/// because nothing ever read a document.
#[tokio::test]
async fn the_receipt_counts_an_artifact_no_run_holds_as_an_artifact_failure() {
    let receipt = receipt_of_failure(
        r#"
name: unheld-fixture
nodes:
  - id: compose
    kind: workflow
    use: producer
    artifacts: { produces: [report.md] }
"#,
        "compose",
        Failure::artifacts(vec![ArtifactFailure::Unheld {
            run: RunId::from("run-child-1"),
            producer: None,
            artifact: ArtifactId::of("report.md", None),
        }]),
    )
    .await;

    assert_eq!(
        receipt.diagnostics,
        vec![DiagnosticCount {
            kind: None,
            code: DiagnosticCode::Artifact(ArtifactCode::Unheld),
            occurrences: 1,
        }],
        "an artifact no run holds is counted by its own code, under no kind"
    );
    assert!(
        render_receipt_markdown(&receipt).contains("`artifact-unheld` 1 time"),
        "{receipt:?}"
    );
}

/// A session node that ended owing the document it declared fails on the
/// artifact itself too: the receipt counts it by its own code, under no
/// kind, because nothing ever read a document either.
#[tokio::test]
async fn the_receipt_counts_a_document_nobody_handed_over_as_an_artifact_failure() {
    let receipt = receipt_of_failure(
        r#"
name: undelivered-fixture
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the plan."
    artifacts:
      produces: [tasks]
"#,
        "plan",
        Failure::artifacts(vec![ArtifactFailure::Undelivered {
            node: NodeId::from("plan"),
            artifact: ArtifactId::of("plan.yaml", Some(ArtifactKind::Tasks)),
        }]),
    )
    .await;

    assert_eq!(
        receipt.diagnostics,
        vec![DiagnosticCount {
            kind: None,
            code: DiagnosticCode::Artifact(ArtifactCode::Undelivered),
            occurrences: 1,
        }],
        "a document nobody handed over is counted by its own code, under no kind"
    );
    assert!(
        render_receipt_markdown(&receipt).contains("`artifact-undelivered` 1 time"),
        "{receipt:?}"
    );
}

/// `duplicate-id` is one rule asked of three documents. Counting it by
/// code alone cannot say whether a run hit three broken tasks documents or one
/// of each, so the count carries the kind and the two never merge.
#[test]
fn the_same_rule_in_two_documents_counts_as_two_facts() {
    let mut receipt = sample_receipt(EventChainStatus::Intact { events: 342 });
    receipt.diagnostics = vec![
        DiagnosticCount {
            kind: Some(ArtifactKind::Tasks.into()),
            code: DiagnosticCode::Rule(RuleCode::DuplicateId),
            occurrences: 3,
        },
        DiagnosticCount {
            kind: Some(ArtifactKind::Findings.into()),
            code: DiagnosticCode::Rule(RuleCode::DuplicateId),
            occurrences: 1,
        },
        DiagnosticCount {
            kind: None,
            code: DiagnosticCode::File(FileCode::Missing),
            occurrences: 1,
        },
    ];
    let markdown = render_receipt_markdown(&receipt);
    assert!(
        markdown.contains("`duplicate-id` in the tasks document 3 times"),
        "{markdown}"
    );
    assert!(
        markdown.contains("`duplicate-id` in the findings artifact 1 time"),
        "{markdown}"
    );
    // A file that was never written has no content whose kind could be
    // wrong, so its count carries the code alone.
    assert!(markdown.contains("`artifact-missing` 1 time"), "{markdown}");
}

/// The same counts as data, so a program reading the JSON receipt never
/// has to parse the sentence above back apart.
#[test]
fn the_json_receipt_carries_the_counts_as_data() {
    let mut receipt = sample_receipt(EventChainStatus::Intact { events: 342 });
    receipt.diagnostics = vec![DiagnosticCount {
        kind: Some(ArtifactKind::Tasks.into()),
        code: DiagnosticCode::Parse(ParseCode::Parse),
        occurrences: 3,
    }];
    let rendered = render_receipt_json(&receipt).expect("the receipt renders");
    let json: serde_json::Value = serde_json::from_str(&rendered).expect("the receipt is JSON");
    assert_eq!(json["diagnostics"][0]["kind"], "tasks");
    assert_eq!(json["diagnostics"][0]["code"], "parse");
    assert_eq!(json["diagnostics"][0]["occurrences"], 3);
}

/// A clean run says nothing about problems, rather than a zero line —
/// the same rule the baseline section already follows.
#[test]
fn a_run_with_no_artifact_problems_prints_no_line_about_them() {
    let receipt = sample_receipt(EventChainStatus::Intact { events: 342 });
    assert!(!render_receipt_markdown(&receipt).contains("artifact problem"));
}
