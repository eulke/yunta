//! Promotion at the engine boundary: the escalation
//! at exhausted re-routes offers a `promote` option exactly when a
//! later mode exists (the modes' declaration order forms a ladder), and
//! choosing it closes *this* run for good — `run_finished` with
//! `terminal_state: Promoted`, never reopened — after recording
//! `promotion_signaled` on the same log. Actually creating and running
//! the successor is `yunta-cli`'s own job (`commands/promote.rs`) —
//! `execute_run` alone only has an *already-prepared* worktree, never
//! the original checkout a fresh one needs — so this suite only proves
//! the parent-side half of the chain.

use yunta_core::events::EventPayload;
use yunta_core::events::{BaselineOrigin, FindingEvent, RunEvent, TaskEvent};
use yunta_core::ModeName;
use yunta_engine::{BirthArtifact, BirthOrigin, HumanInteraction, RunReport, RunTerminal};
use yunta_testkit::{baselines, Bench, ScriptedInteraction};
use yunta_testkit_core::{Log, SeqIdSource};

/// The fixture every run of this suite is driven on: no session is
/// scripted, because every node of these workflows runs a command.
const NO_SESSIONS: &str = "sessions: []\n";

/// `lint` fails immediately (`test -f` on a file nothing ever creates)
/// with `max_reroutes: 0` — its very first failure already exhausts
/// re-routes, so the escalation gate fires on the first wake.
const PROMOTABLE_WORKFLOW: &str = r#"
name: promotable
modes:
  quick:  { include: [lint, fix-lint] }
  full:   { include: all }
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "true"
"#;

const NO_LATER_MODE_WORKFLOW: &str = r#"
name: promotable
modes:
  full:   { include: all }
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "true"
"#;

/// A run closed with engine findings planted on its log after creation —
/// the scenario where a scope-expansion denial lives only in the parent's
/// events.
async fn run_with_mode_and_findings(
    workflow_yaml: &str,
    mode: &str,
    interaction: &dyn HumanInteraction,
    findings: &[yunta_core::events::Finding],
) -> (Bench, RunTerminal) {
    run_planted(
        workflow_yaml,
        mode,
        interaction,
        Planted {
            findings,
            ..Planted::default()
        },
    )
    .await
}

/// What a test states about the predecessor before it executes: engine
/// findings on its log, and a tasks document it is born holding with
/// some of those tasks already done.
#[derive(Default)]
struct Planted<'a> {
    findings: &'a [yunta_core::events::Finding],
    /// The tasks document the run is born holding, as a person writes
    /// one.
    tasks: Option<&'a str>,
    /// The ids of `tasks` the predecessor's log leaves `done`.
    done: &'a [&'a str],
}

/// Runs one workflow to its close with `planted` already true of it,
/// answering with the world the closed run leaves behind.
async fn run_planted(
    workflow_yaml: &str,
    mode: &str,
    interaction: &dyn HumanInteraction,
    planted: Planted<'_>,
) -> (Bench, RunTerminal) {
    let born: Vec<BirthArtifact> = planted
        .tasks
        .into_iter()
        .map(|tasks| BirthArtifact {
            artifact: yunta_core::events::ArtifactId::Interpreted {
                kind: yunta_core::ArtifactKind::Tasks,
            },
            origin: BirthOrigin::Input {
                input: "tasks".into(),
            },
            bytes: canonical_tasks(tasks),
        })
        .collect();
    let bench = Bench::new().in_mode(mode).born_holding(born);
    let RunReport { terminal, .. } = bench
        .run_sabotaged_answering(workflow_yaml, NO_SESSIONS, interaction, |_| {
            plant(&bench, &planted)
        })
        .await;
    (bench, terminal)
}

/// States `planted` on the run's log, between its creation — which
/// registers the tasks a `done` points back to — and its first wake.
fn plant(bench: &Bench, planted: &Planted<'_>) {
    // Where the planted work landed: this run's own tree, which is what
    // a successor branches from, so the `done` answers there too.
    let landed: yunta_core::CommitSha =
        yunta_testkit::git_output(&bench.worktree, &["rev-parse", "HEAD"])
            .parse()
            .unwrap();
    for task in planted.done {
        let caused_by = registration_of(&bench.events(), task);
        record(
            bench,
            yunta_testkit::task_status_changed(
                &(*task).into(),
                yunta_core::events::TaskStatus::Done,
                Some(&landed),
                caused_by,
            ),
        );
    }

    for finding in planted.findings {
        record(
            bench,
            EventPayload::Findings(FindingEvent::Posted(
                yunta_core::events::FindingPostedPayload {
                    finding: finding.clone(),
                },
            )),
        );
    }
}

/// Appends one run-level event to the run's log.
fn record(bench: &Bench, payload: EventPayload) {
    bench
        .storage
        .append(
            &yunta_core::events::EventDraft {
                run_id: bench.run_id.clone(),
                node_id: None,
                payload,
            },
            &yunta_core::SystemClock,
        )
        .unwrap();
}

#[tokio::test]
async fn promote_is_offered_and_closes_the_run_with_promotion_signaled() {
    let interaction = ScriptedInteraction::choose("promote");
    let bench = Bench::new().in_mode("quick");
    let RunReport { terminal, .. } = bench
        .run_with_interaction(PROMOTABLE_WORKFLOW, NO_SESSIONS, &interaction)
        .await;
    let events = bench.events();

    match &terminal {
        RunTerminal::Promoted { suggested_mode } => assert_eq!(suggested_mode, "full"),
        other => panic!("expected Promoted, got {other:?}"),
    }

    let signaled = events.iter().find_map(|e| match e.payload() {
        Some(EventPayload::Run(RunEvent::PromotionSignaled(p))) => Some(p),
        _ => None,
    });
    let signaled = signaled.expect("promotion_signaled must be on the parent's own log");
    assert_eq!(signaled.suggested_mode, "full");
    assert!(!signaled.reason.is_empty());

    let finished = events.iter().find_map(|e| match e.payload() {
        Some(EventPayload::Run(RunEvent::Finished(p))) => Some(p),
        _ => None,
    });
    assert_eq!(
        finished.map(|p| p.terminal_state),
        Some(yunta_core::events::TerminalState::Promoted)
    );

    // Promoting closes the run for good — no further events after
    // run_finished (nothing reopens a finished run).
    let last = events.last().unwrap();
    assert!(matches!(
        last.payload(),
        Some(EventPayload::Run(RunEvent::Finished(_)))
    ));

    // The offered options actually included "promote" — proving the
    // escalation added it, not that this test just got lucky with a
    // fallback.
    let options = interaction.seen_options();
    assert!(options[0].contains(&"promote".into()), "got: {options:?}");
}

#[tokio::test]
async fn promote_is_never_offered_with_no_later_mode() {
    let interaction = ScriptedInteraction::choose("abort");
    let RunReport { terminal, .. } = Bench::new()
        .in_mode("full")
        .run_with_interaction(NO_LATER_MODE_WORKFLOW, NO_SESSIONS, &interaction)
        .await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));

    let options = interaction.seen_options();
    assert!(
        !options[0].contains(&"promote".into()),
        "the last declared mode has nowhere to promote to — got: {options:?}"
    );
}

#[tokio::test]
async fn without_a_live_human_interaction_the_run_just_pauses_never_promotes() {
    let bench = Bench::new().in_mode("quick");
    let RunReport { terminal, .. } = bench.run(PROMOTABLE_WORKFLOW, NO_SESSIONS).await;
    let events = bench.events();
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(EventPayload::Run(RunEvent::PromotionSignaled(_)))
        )),
        "no live surface to choose promote from — must never happen on its own"
    );
}

// --- findings survive promotion ---------------------------------------

fn finding(id: &str, title: &str, location: &str) -> yunta_core::events::Finding {
    yunta_core::events::Finding {
        id: id.into(),
        severity: yunta_core::events::FindingSeverity::Major,
        title: title.to_string(),
        location: location.into(),
        detail: "scope expansion denied by a human".to_string(),
        proposed_criterion: None,
    }
}

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
        run_with_mode_and_findings(PROMOTABLE_WORKFLOW, "quick", &interaction, &planted).await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));

    let path = bench.run_dir().join("artifacts/findings.yaml");
    let bytes = std::fs::read(&path).expect("the promotion close must derive the file");
    // Through the same door every findings artifact is read by, so the
    // derived file satisfies the shape and the rules, not just serde.
    let file =
        yunta_core::shape::read::<yunta_core::FindingsFile>(&bytes, "artifacts/findings.yaml")
            .expect("the derived file must satisfy the findings door");
    assert_eq!(file.findings.len(), 2, "duplicates collapse: {file:?}");
    assert_eq!(file.findings[0].id, "scope-expansion-T001-1");
    assert_eq!(file.findings[1].id, "scope-expansion-T002-1");
}

#[tokio::test]
async fn a_promoting_run_with_no_findings_writes_no_inherited_file() {
    let interaction = ScriptedInteraction::choose("promote");
    let bench = Bench::new().in_mode("quick");
    let RunReport { terminal, .. } = bench
        .run_with_interaction(PROMOTABLE_WORKFLOW, NO_SESSIONS, &interaction)
        .await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));
    assert!(
        !bench.run_dir().join("artifacts/findings.yaml").exists(),
        "no findings, no file — zero noise"
    );
}

/// `create_promotion_successor` with everything this suite's world
/// fixes: the `full` mode every test promotes into, the bench's roots,
/// its clock and the ids the successor is minted from. Creating a
/// successor is the CLI's job, never `execute_run`'s, so every test
/// here does it by hand — through this one door.
async fn successor_of(
    predecessor: yunta_engine::Predecessor<'_>,
    bench: &Bench,
    ids: &SeqIdSource,
) -> yunta_engine::PromotionSuccessor {
    let repo = predecessor.worktree.to_path_buf();
    yunta_engine::create_promotion_successor(
        predecessor,
        &repo,
        &ModeName::from("full"),
        yunta_engine::RunRoots {
            runs: &bench.runs_root,
            worktrees: &bench.runs_root.parent().unwrap().join("worktrees"),
        },
        yunta_engine::CallerInfra {
            storage: &bench.storage.async_handle(),
            ids,
            supervision: bench.supervision(),
        },
    )
    .await
    .expect("the successor is created")
}

/// The run `bench` drove, as the predecessor of a promotion.
fn predecessor_of<'a>(
    bench: &'a Bench,
    manifest: &'a yunta_core::Manifest,
    run_dir: &'a std::path::Path,
) -> yunta_engine::Predecessor<'a> {
    yunta_engine::Predecessor {
        id: &bench.run_id,
        manifest,
        worktree: &bench.worktree,
        run_dir,
    }
}

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
    let bench = Bench::new().in_mode("quick");
    tokio::fs::write(bench.worktree.join("marker.txt"), "ok\n")
        .await
        .unwrap();
    let interaction = ScriptedInteraction::choose("promote");
    let RunReport { terminal, .. } = bench
        .run_full(
            PROMOTABLE_WORKFLOW,
            NO_SESSIONS,
            CONFIG_WITH_BASELINE,
            &interaction,
        )
        .await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));
    let measured = baselines(&bench.events());

    let manifest = bench.manifest();
    let run_dir = bench.run_dir();
    let ids = SeqIdSource::new("minted");
    let successor = successor_of(predecessor_of(&bench, &manifest, &run_dir), &bench, &ids).await;

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
        run_with_mode_and_findings(PROMOTABLE_WORKFLOW, "quick", &interaction, &planted).await;
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
        run_with_mode_and_findings(PROMOTABLE_WORKFLOW, "quick", &interaction, &planted).await;
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

/// The tasks document a run is born holding, rendered the way the run
/// stores it: canonical, so the successor inherits the same bytes.
fn canonical_tasks(yaml: &str) -> Vec<u8> {
    let document: yunta_core::TasksFile =
        yunta_core::shape::read(yaml.as_bytes(), "tasks").expect("a valid tasks document");
    yunta_core::shape::render(&document)
        .expect("the canonical rendering")
        .into_bytes()
}

/// The position of `task`'s registration on `events` — what a
/// `task_status_changed` about it points back to.
fn registration_of(events: &[yunta_core::events::StoredEvent], task: &str) -> yunta_core::Seq {
    events
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Tasks(TaskEvent::Registered(p))) if p.task_id.as_str() == task => {
                Some(event.seq)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no task_registered for `{task}`: {events:?}"))
}

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
        "quick",
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
        "quick",
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

    assert_eq!(
        yunta_engine::inherited_findings(&events),
        yunta_engine::dedup_findings(&standing),
        "a successor inherits the set the run's own frame counts",
    );
    assert_eq!(yunta_engine::inherited_findings(&events).len(), 2);
}

#[tokio::test]
async fn a_promotion_successor_is_stamped_by_the_run_clock() {
    // Deciding is a pure function of what it is handed, and time is
    // handed in: a successor carries the caller's reading of the clock,
    // never the wall clock read behind its back. A chain whose members
    // disagree about when they happened is a chain nobody can replay.
    let interaction = ScriptedInteraction::choose("promote");
    let bench = Bench::new().in_mode("quick");
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
