//! A successor is born holding what its predecessor built: its measurement,
//! its artifacts, its tasks and its checkout.

use super::*;

/// The config every baseline test of this suite runs under: a suite
/// that passes on the tree the bench stands up.
const CONFIG_WITH_BASELINE: &str = "\
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
baseline:
  suite: \"cat marker.txt\"
";

/// A promotion is the invocation carrying on: the successor asks the
/// same question about the same tree its predecessor started from, so it
/// is born holding that measurement instead of taking one of the tree
/// its predecessor already worked.
#[tokio::test]
async fn a_successor_is_born_holding_its_predecessors_measurement() {
    let bench = Bench::new().in_mode("alpha");
    tokio::fs::write(bench.worktree.join("marker.txt"), "ok\n")
        .await
        .unwrap();
    born_holding_the_measurement(&bench).await;
}

/// The same for a run that measures aside, while its nodes run: the
/// question that may promote it waits for the measurement it hands on.
#[tokio::test]
async fn a_successor_of_a_run_measuring_aside_is_born_holding_its_measurement() {
    let bench = Bench::new().in_mode("alpha");
    tokio::fs::write(bench.worktree.join("marker.txt"), "ok\n")
        .await
        .unwrap();
    yunta_testkit::git(&bench.worktree, &["add", "marker.txt"]);
    yunta_testkit::git(&bench.worktree, &["commit", "-q", "-m", "the marker"]);
    born_holding_the_measurement(&bench).await;
}

/// Promotes the run `bench` drives and checks its successor is born
/// holding the measurement it took.
async fn born_holding_the_measurement(bench: &Bench) {
    let interaction = ScriptedInteraction::choose("promote");
    // The comparison is the later mode's; the lineage measures at its
    // root because the workflow reads the measurement somewhere.
    let compared = format!(
        "{PROMOTABLE_WORKFLOW}  - {{ id: regressions, kind: check, builtin: baseline_compare, depends_on: [lint] }}\n"
    );
    let RunReport { terminal, .. } = bench
        .run_full(&compared, NO_SESSIONS, CONFIG_WITH_BASELINE, &interaction)
        .await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));
    let measured = baselines(&bench.events());

    let manifest = bench.manifest();
    let run_dir = bench.run_dir();
    let ids = SeqIdSource::new("minted");
    let successor = successor_of(predecessor_of(bench, &manifest, &run_dir), bench, &ids).await;

    let held = baselines(&bench.storage.events_for_run(&successor.run_id).unwrap());
    assert_eq!(held.len(), 1, "the successor is born holding a measurement");
    assert_eq!(
        held[0].origin,
        BaselineOrigin::Inherited {
            run: bench.run_id.clone()
        },
        "and the origin names the run that took it"
    );
    assert_eq!(held[0].command, measured[0].command);
    assert_eq!(held[0].hash, measured[0].hash);
    assert!(
        !yunta_engine::run_dir::baseline_capture(&successor.run_dir).exists(),
        "the bytes stay with the run that measured"
    );
}

#[tokio::test]
async fn a_successor_is_born_naming_every_artifact_it_inherits() {
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
    assert!(
        matches!(
            events.first().and_then(|e| e.payload()),
            Some(EventPayload::Run(RunEvent::Created(_)))
        ),
        "the successor exists in its log before anything is said about it"
    );
    let inherited = yunta_testkit::accepted(&events);
    assert_eq!(
        inherited.len(),
        1,
        "one acceptance per inherited file: {inherited:?}"
    );
    let held = &inherited[0];
    assert_eq!(
        held.producer, None,
        "no node of the successor produced it — it was handed over"
    );
    assert_eq!(
        held.artifact,
        yunta_core::events::ArtifactId::Interpreted {
            kind: yunta_core::ArtifactKind::Findings
        },
        "the identity the predecessor held it under, not the file name"
    );
    assert_eq!(
        held.origin,
        yunta_core::events::RecordedOrigin::Inherited {
            run: bench.run_id.clone(),
            producer: None,
        },
        "the predecessor derived it as the run's own, with no node behind it"
    );
    assert_eq!(
        std::fs::read(
            successor
                .run_dir
                .join("objects")
                .join(held.content_hash.as_str())
        )
        .expect("the successor holds the bytes"),
        std::fs::read(run_dir.join("artifacts/findings.yaml")).unwrap()
    );
}

#[tokio::test]
async fn a_successor_inherits_what_the_log_holds_and_not_a_stray_file() {
    // A file nobody declared is not an artifact: no acceptance accounts
    // for it, so the predecessor does not hold it and the successor is
    // not born with it.
    let interaction = ScriptedInteraction::choose("promote");
    let planted = [finding("scope-expansion-T001-1", "Denied", "tasks/T001")];
    let (bench, terminal) =
        run_with_mode_and_findings(PROMOTABLE_WORKFLOW, "alpha", &interaction, &planted).await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));

    let manifest = bench.manifest();
    let run_dir = bench.run_dir();
    std::fs::write(run_dir.join("artifacts/stray.md"), "nobody declared this\n").unwrap();

    let ids = SeqIdSource::new("minted");
    let successor = successor_of(predecessor_of(&bench, &manifest, &run_dir), &bench, &ids).await;

    let events = bench.storage.events_for_run(&successor.run_id).unwrap();
    let inherited = yunta_testkit::accepted(&events);
    assert_eq!(
        inherited
            .iter()
            .map(|held| held.artifact.to_string())
            .collect::<Vec<_>>(),
        vec!["findings".to_string()],
        "only what the predecessor's log holds is inherited: {inherited:?}"
    );
    assert!(
        !successor.run_dir.join("artifacts/stray.md").exists(),
        "a stray file is not a fact of the predecessor, so it reaches no successor"
    );
}

// --- a successor owns the tasks its predecessor was working on --------

const TWO_TASKS: &str = r#"
tasks:
  - id: T001
    title: "Write a"
    scope: ["a.txt"]
    criteria: [{cmd: "test -f a.txt"}]
  - id: T002
    title: "Write b"
    scope: ["b.txt"]
    criteria: [{cmd: "test -f b.txt"}]
"#;

#[tokio::test]
async fn a_successor_is_born_owning_its_predecessor_s_tasks_with_the_done_ones_done() {
    let interaction = ScriptedInteraction::choose("promote");
    let (bench, terminal) = run_planted(
        PROMOTABLE_WORKFLOW,
        "alpha",
        &interaction,
        Planted {
            tasks: Some(TWO_TASKS),
            done: &["T001"],
            ..Planted::default()
        },
    )
    .await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));

    let manifest = bench.manifest();
    let run_dir = bench.run_dir();
    let ids = SeqIdSource::new("minted");
    let successor = successor_of(predecessor_of(&bench, &manifest, &run_dir), &bench, &ids).await;

    let events = bench.storage.events_for_run(&successor.run_id).unwrap();
    let inherited = yunta_testkit::accepted(&events);
    assert_eq!(
        inherited
            .iter()
            .map(|held| held.artifact.to_string())
            .collect::<Vec<_>>(),
        vec!["tasks".to_string()],
    );
    assert_eq!(
        inherited[0].origin,
        yunta_core::events::RecordedOrigin::Inherited {
            run: bench.run_id.clone(),
            producer: None,
        },
        "the predecessor held the document with no node behind it"
    );

    let registered: Vec<String> = events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Tasks(TaskEvent::Registered(p))) => Some(p.task_id.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        registered,
        vec!["T001".to_string(), "T002".to_string()],
        "the successor says what it has to do about every task it was handed"
    );

    let changes: Vec<(String, yunta_core::events::TaskStatus, yunta_core::Seq)> = events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Tasks(TaskEvent::StatusChanged(p))) => {
                Some((p.task_id.to_string(), p.new_status, p.caused_by))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        changes.len(),
        1,
        "only what the predecessor finished crosses: {changes:?}"
    );
    assert_eq!(changes[0].0, "T001");
    assert_eq!(changes[0].1, yunta_core::events::TaskStatus::Done);
    assert_eq!(
        changes[0].2,
        registration_of(&events, "T001"),
        "the registration is what caused the status the successor is born with"
    );

    let state = yunta_engine::derive(&events);
    assert_eq!(
        state.tasks.status("T001"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.status("T002"),
        Some(yunta_core::events::TaskStatus::Pending)
    );
}

#[tokio::test]
async fn a_done_that_crossed_keeps_the_commit_it_names_so_it_crosses_again() {
    let interaction = ScriptedInteraction::choose("promote");
    let (bench, terminal) = run_planted(
        PROMOTABLE_WORKFLOW,
        "alpha",
        &interaction,
        Planted {
            tasks: Some(TWO_TASKS),
            done: &["T001"],
            ..Planted::default()
        },
    )
    .await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));

    let manifest = bench.manifest();
    let run_dir = bench.run_dir();
    let ids = SeqIdSource::new("minted");
    let first = successor_of(predecessor_of(&bench, &manifest, &run_dir), &bench, &ids).await;

    // The second link of the chain: the run the first successor was
    // born as is itself promoted, and what it says about T001 is only
    // what it was born saying.
    let second = successor_of(
        yunta_engine::Predecessor {
            id: &first.run_id,
            manifest: &first.manifest,
            worktree: &first.worktree,
            run_dir: &first.run_dir,
        },
        &bench,
        &ids,
    )
    .await;

    let landed = done_at(
        &bench.storage.events_for_run(&first.run_id).unwrap(),
        "T001",
    );
    assert_eq!(
        done_at(
            &bench.storage.events_for_run(&second.run_id).unwrap(),
            "T001"
        ),
        landed,
        "a done that crossed names the same commit further down the chain, which is what          lets the third link answer the question the first one did"
    );
    assert!(
        landed.is_some(),
        "the commit the work landed at is what the crossing is made of"
    );

    let state = yunta_engine::derive(&bench.storage.events_for_run(&second.run_id).unwrap());
    assert_eq!(
        state.tasks.status("T001"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.status("T002"),
        Some(yunta_core::events::TaskStatus::Pending)
    );
}

/// The commit the last `done` about `task` on `events` names.
fn done_at(
    events: &[yunta_core::events::StoredEvent],
    task: &str,
) -> Option<yunta_core::CommitSha> {
    events
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Tasks(TaskEvent::StatusChanged(p)))
                if p.task_id.as_str() == task
                    && p.new_status == yunta_core::events::TaskStatus::Done =>
            {
                Some(p.commit.clone())
            }
            _ => None,
        })
        .flatten()
}

#[tokio::test]
async fn a_promotion_successor_is_stamped_by_the_run_clock() {
    // Deciding is a pure function of what it is handed, and time is
    // handed in: a successor carries the caller's reading of the clock,
    // never the wall clock read behind its back. A chain whose members
    // disagree about when they happened is a chain nobody can replay.
    let interaction = ScriptedInteraction::choose("promote");
    let bench = Bench::new().in_mode("alpha");
    let RunReport { terminal, .. } = bench
        .run_with_interaction(PROMOTABLE_WORKFLOW, NO_SESSIONS, &interaction)
        .await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));

    let manifest = bench.manifest();
    let run_dir = bench.run_dir();
    let ids = SeqIdSource::new("minted");
    let successor = successor_of(predecessor_of(&bench, &manifest, &run_dir), &bench, &ids).await;

    let events = bench.storage.events_for_run(&successor.run_id).unwrap();
    let born = events.first().expect("the successor's own birth");
    assert_eq!(
        born.timestamp,
        chrono::DateTime::parse_from_rfc3339(yunta_testkit_core::FIXED_NOW)
            .unwrap()
            .with_timezone(&chrono::Utc),
        "the clock the caller handed in, not the one on the wall"
    );
}

/// A successor carries on in its predecessor's own checkout, on a branch of
/// its own cut where that tree stands: what the predecessor built is there,
/// and the successor's birth names that checkout.
#[tokio::test]
async fn successor_works_in_its_predecessors_checkout() {
    let bench = Bench::new().in_mode("alpha");
    let interaction = ScriptedInteraction::choose("promote");
    let RunReport { terminal, .. } = bench
        .run_with_interaction(PROMOTABLE_WORKFLOW, NO_SESSIONS, &interaction)
        .await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));
    let (manifest, run_dir) = (bench.manifest(), bench.run_dir());
    let ids = SeqIdSource::new("minted");

    let successor = successor_of(predecessor_of(&bench, &manifest, &run_dir), &bench, &ids).await;

    assert_eq!(successor.worktree, bench.worktree);
    let on = yunta_testkit::git_output(&bench.worktree, &["branch", "--show-current"]);
    assert_eq!(on.trim(), format!("yunta/run/{}", successor.run_id));
    let born = bench.storage.events_for_run(&successor.run_id).unwrap();
    let named = yunta_engine::derive(&born)
        .run
        .checkout()
        .map(std::path::Path::to_path_buf);
    assert_eq!(named, Some(bench.worktree.clone()));
}
