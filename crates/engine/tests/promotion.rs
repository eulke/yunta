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

#[path = "promotion/findings.rs"]
mod findings;
#[path = "promotion/successor.rs"]
mod successor;

use yunta_core::events::EventPayload;
use yunta_core::events::{
    BaselineOrigin, Failure, FindingCause, FindingEvent, FindingSays, RunEvent, ScopeAsker,
    TaskEvent,
};
use yunta_core::ModeName;
use yunta_engine::{BirthArtifact, BirthOrigin, HumanInteraction, RunReport, RunTerminal};
use yunta_testkit::{baselines, Bench, ScriptedInteraction};
use yunta_testkit_core::{FixedClock, Log, SeqIdSource};

/// The fixture every run of this suite is driven on: no session is
/// scripted, because every node of these workflows runs a command.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// `lint` fails immediately (`test -f` on a file nothing ever creates)
/// with `max_reroutes: 0` — its very first failure already exhausts
/// re-routes, so the escalation gate fires on the first wake.
const PROMOTABLE_WORKFLOW: &str = r#"
name: promotable
modes:
  alpha:  { include: [lint, fix-lint] }
  beta:   { include: all }
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
  beta:   { include: all }
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
            &yunta_core::events::EventDraft::new(bench.run_id.clone(), None, payload)
                .expect("a draft names its subject"),
            &FixedClock,
        )
        .unwrap();
}

#[tokio::test]
async fn promote_is_offered_and_closes_the_run_with_promotion_signaled() {
    let interaction = ScriptedInteraction::choose("promote");
    let bench = Bench::new().in_mode("alpha");
    let RunReport { terminal, .. } = bench
        .run_with_interaction(PROMOTABLE_WORKFLOW, NO_SESSIONS, &interaction)
        .await;
    let events = bench.events();

    match &terminal {
        RunTerminal::Promoted { suggested_mode } => assert_eq!(suggested_mode, "beta"),
        other => panic!("expected Promoted, got {other:?}"),
    }

    let signaled = events.iter().find_map(|e| match e.payload() {
        Some(EventPayload::Run(RunEvent::PromotionSignaled(p))) => Some(p),
        _ => None,
    });
    let signaled = signaled.expect("promotion_signaled must be on the parent's own log");
    assert_eq!(signaled.suggested_mode, "beta");
    let cause = signaled.cause.as_ref().expect("a promotion with a cause");
    assert_eq!(cause.node.as_str(), "lint");
    assert_eq!(cause.goto.as_str(), "fix-lint");
    let command_exit = matches!(cause.failure, Some(Failure::Exited { .. }));
    assert!(command_exit, "{:?}", cause.failure);

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
        .in_mode("beta")
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
    let bench = Bench::new().in_mode("alpha");
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
        says: FindingSays::Written {
            title: title.to_string(),
            detail: "scope expansion denied by a human".to_string(),
        },
        location: location.into(),
        proposed_criterion: None,
    }
}

/// The title a session wrote a finding under, which these tests compare by.
fn written_title(finding: &yunta_core::events::Finding) -> String {
    match &finding.says {
        FindingSays::Written { title, .. } => title.clone(),
        FindingSays::Found { cause } => panic!("a session's finding, not {cause:?}"),
    }
}

/// The id a held findings entry is declared under, whichever way it says itself.
fn held_id(entry: &yunta_core::HeldEntry) -> String {
    match entry {
        yunta_core::HeldEntry::Written(written) => written.id.to_string(),
        yunta_core::HeldEntry::Found(found) => found.id.to_string(),
    }
}

/// `create_promotion_successor` with everything this suite's world
/// fixes: the `beta` mode every test promotes into, the bench's roots,
/// its clock and the ids the successor is minted from. Creating a
/// successor is the CLI's job, never `execute_run`'s, so every test
/// here does it by hand — through this one door.
async fn successor_of(
    predecessor: yunta_engine::Predecessor<'_>,
    bench: &Bench,
    ids: &SeqIdSource,
) -> yunta_engine::PromotionSuccessor {
    yunta_engine::create_promotion_successor(
        predecessor,
        &ModeName::from("beta"),
        yunta_engine::RunRoots {
            runs: &bench.runs_root,
            worktrees: &bench.runs_root.parent().unwrap().join("worktrees"),
        },
        yunta_engine::CallerInfra {
            storage: &bench.storage.async_handle(),
            ids,
            supervision: bench.supervision(),
            environment: None,
            secrets: &*yunta_testkit::no_secrets(),
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
