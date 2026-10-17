//! Findings survive promotion: a successor inherits the findings its
//! predecessor left open, each as the run's own words or as the cause the
//! engine found, and a denial it holds reaches it as that cause.

use super::*;

#[tokio::test]
async fn a_promoting_run_derives_findings_inherited_for_its_successor() {
    let interaction = ScriptedInteraction::choose("promote");
    let planted = [
        finding(
            "scope-expansion-T001-1",
            "Scope expansion denied",
            "tasks/T001",
        ),
        // Same location + same title modulo case/whitespace: a duplicate
        // under the normative dedup rule.
        finding(
            "scope-expansion-T001-2",
            "scope  expansion DENIED",
            "tasks/T001",
        ),
        finding(
            "scope-expansion-T002-1",
            "Scope expansion denied",
            "tasks/T002",
        ),
    ];
    let (bench, terminal) =
        run_with_mode_and_findings(PROMOTABLE_WORKFLOW, "alpha", &interaction, &planted).await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));

    let path = bench.run_dir().join("artifacts/findings.yaml");
    let bytes = std::fs::read(&path).expect("the promotion close must derive the file");
    // Through the same door every findings artifact is read by, so the
    // derived file satisfies the shape and the rules, not just serde.
    let file =
        yunta_core::shape::read::<yunta_core::HeldFindings>(&bytes, "artifacts/findings.yaml")
            .expect("the derived file must satisfy the findings door");
    assert_eq!(file.findings.len(), 2, "duplicates collapse: {file:?}");
    assert_eq!(held_id(&file.findings[0]), "scope-expansion-T001-1");
    assert_eq!(held_id(&file.findings[1]), "scope-expansion-T002-1");
}

#[tokio::test]
async fn a_promoting_run_with_no_findings_writes_no_inherited_file() {
    let interaction = ScriptedInteraction::choose("promote");
    let bench = Bench::new().in_mode("alpha");
    let RunReport { terminal, .. } = bench
        .run_with_interaction(PROMOTABLE_WORKFLOW, NO_SESSIONS, &interaction)
        .await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));
    assert!(
        !bench.run_dir().join("artifacts/findings.yaml").exists(),
        "no findings, no file — zero noise"
    );
}

#[test]
fn inherited_findings_dedup_the_way_the_frame_counts_them() {
    use yunta_core::events::FindingPostedPayload;
    let posted = |finding: yunta_core::events::Finding| {
        EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload { finding }))
    };
    // Two reviewers complaining about the same place, spelled apart by
    // case and by the space between two words.
    let events = Log::for_run("run-x")
        .node(
            "review-a",
            posted(finding("f1", "Scope  expansion DENIED", "tasks/T001")),
        )
        .node(
            "review-b",
            posted(finding("f2", "scope expansion denied", "tasks/T001")),
        )
        .node(
            "review-b",
            posted(finding("f3", "a second complaint", "src/lib.rs")),
        )
        .build();

    let standing: Vec<yunta_core::events::Finding> =
        yunta_core::events::findings::effective(&events)
            .into_iter()
            .map(|posted| posted.finding)
            .collect();
    let counted: Vec<String> = yunta_engine::dedup_findings(&standing)
        .iter()
        .map(written_title)
        .collect();

    let inherited = yunta_engine::inherited_findings(&events);
    assert_eq!(
        inherited.iter().map(written_title).collect::<Vec<_>>(),
        counted,
        "a successor inherits the set the run's own frame counts",
    );
    assert_eq!(
        inherited
            .iter()
            .map(|finding| finding.id.to_string())
            .collect::<Vec<_>>(),
        ["review-a/f1", "review-b/f3"],
        "each under the node that reported it: ids are unique only within one node"
    );
}

#[tokio::test]
async fn a_successor_holds_its_predecessors_unsettled_findings_as_the_runs_own() {
    let interaction = ScriptedInteraction::choose("promote");
    let planted = [finding("scope-expansion-T001-1", "Denied", "tasks/T001")];
    let (bench, terminal) =
        run_with_mode_and_findings(PROMOTABLE_WORKFLOW, "alpha", &interaction, &planted).await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));

    let manifest = bench.manifest();
    let run_dir = bench.run_dir();
    let ids = SeqIdSource::new("minted");
    let successor = successor_of(predecessor_of(&bench, &manifest, &run_dir), &bench, &ids).await;

    let events = bench.storage.events_for_run(&successor.run_id).unwrap();
    let standing = yunta_core::events::findings::FindingLedger::of(&events).standing();
    let held: Vec<(Option<String>, String)> = standing
        .findings
        .iter()
        .map(|found| {
            (
                found.node.as_ref().map(ToString::to_string),
                found.finding.id.to_string(),
            )
        })
        .collect();
    assert_eq!(
        held,
        [(None, "scope-expansion-T001-1".to_string())],
        "the run's own, so its gates show it and a person can settle it"
    );
    assert!(
        !yunta_engine::derive(&events).woken(),
        "holding what it was born with is no wake"
    );
}

/// A denial the predecessor holds reaches its successor as the cause the
/// engine found, with no words written for it.
#[tokio::test]
async fn a_successor_holds_an_inherited_denial_as_its_cause() {
    let interaction = ScriptedInteraction::choose("promote");
    let cause = FindingCause::ScopeDenied {
        asker: ScopeAsker::Task("T001".into()),
        denial: yunta_core::expansion::Denial::ModeDeny,
        asked: None,
    };
    let planted = [yunta_core::events::Finding {
        id: "scope-expansion-T001-1".into(),
        severity: yunta_core::events::FindingSeverity::Minor,
        says: FindingSays::Found {
            cause: cause.clone(),
        },
        location: "tasks/T001".into(),
        proposed_criterion: None,
    }];
    let (bench, terminal) =
        run_with_mode_and_findings(PROMOTABLE_WORKFLOW, "alpha", &interaction, &planted).await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));

    let manifest = bench.manifest();
    let run_dir = bench.run_dir();
    let ids = SeqIdSource::new("minted");
    let successor = successor_of(predecessor_of(&bench, &manifest, &run_dir), &bench, &ids).await;

    let events = bench.storage.events_for_run(&successor.run_id).unwrap();
    let inherited = events
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Findings(FindingEvent::Posted(posted))) => {
                Some(posted.finding.clone())
            }
            _ => None,
        })
        .expect("the successor posts the finding it inherits");
    assert_eq!(inherited.says, FindingSays::Found { cause });
    let written = serde_json::to_value(&inherited).expect("a finding writes");
    assert!(
        written.get("title").is_none() && written.get("detail").is_none(),
        "{written}"
    );
}

#[test]
fn two_nodes_findings_under_one_id_are_inherited_as_two() {
    use yunta_core::events::FindingPostedPayload;
    let posted = |finding: yunta_core::events::Finding| {
        EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload { finding }))
    };
    let events = Log::for_run("run-x")
        .node("review@a", posted(finding("f-1", "a race", "src/a.rs")))
        .node("review@b", posted(finding("f-1", "a leak", "src/b.rs")))
        .build();

    let inherited = yunta_engine::inherited_findings(&events);
    let ids: Vec<String> = inherited.iter().map(|f| f.id.to_string()).collect();
    assert_eq!(ids, ["review@a/f-1", "review@b/f-1"]);
    let file = yunta_core::HeldFindings::from_findings(inherited);
    assert!(
        yunta_core::shape::Document::check(&file).is_empty(),
        "the document the successor inherits never repeats an id"
    );
}
