//! A person who changes the tree a failed node stopped on is offered a retry
//! for a cause the tree can change. A wake after the change records the tree,
//! and the menu offers the retry only while the tree differs from the one the
//! node failed on: a check that would answer the same way on the same tree
//! offers abort alone.

use std::path::{Path, PathBuf};
use std::time::Duration;

use yunta_core::events::{
    EventPayload, NodeEvent, Phase, Precondition, RunEvent, SessionEvent, StoredEvent,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::TreeId;
use yunta_engine::{current_escalation, RunReport, RunTerminal};
use yunta_testkit::{after, git, write, Bench, MOCK_CONFIG};

mod common;
use common::*;

/// The menu the run is parked on, by option id.
async fn options(bench: &Bench) -> Vec<String> {
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
    .expect("a parked run is a decision");
    escalation
        .options()
        .iter()
        .map(|option| option.id.to_string())
        .collect()
}

/// What the retries of the failure menu the run is parked on wait for.
async fn retries_wait_for(bench: &Bench) -> Vec<Precondition> {
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
    .expect("a parked run is a decision");
    let menu = escalation
        .into_payload()
        .failure()
        .cloned()
        .expect("a failure's escalation carries its menu");
    menu.remedies
        .into_iter()
        .filter(|remedy| remedy.option == ReservedOption::Retry)
        .filter_map(|remedy| remedy.precondition)
        .collect()
}

/// `node`'s session openings and criteria checks, in the order the log
/// holds them.
fn sessions_and_checks(events: &[StoredEvent], node: &str) -> Vec<&'static str> {
    events
        .iter()
        .filter(|event| event.node_id.as_ref().map(|id| id.as_str()) == Some(node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Session(SessionEvent::Opened(_))) => Some("session"),
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(p))) => Some(match p.phase {
                Phase::Pre => "pre",
                Phase::Post => "post",
            }),
            _ => None,
        })
        .collect()
}

/// A plan that hands one task to the executor, which checks as it goes.
const CHECKED_AS_IT_GOES: &str = r#"
name: checked-as-it-goes
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

/// The plan, then one attempt that leaves the task's criterion red.
fn missed_fixture() -> String {
    plan_session(&format!(
        "tasks:\n{}",
        task_yaml("task-1", "t1", "a1.txt", "test -f a1.txt")
    )) + "  - match_prompt_contains: \"task-1\"\n    outcome: { type: completed, summary: missed }\n"
}

/// One attempt that makes the criterion pass.
const MADE_IT: &str = "dialect: claude-code\nsessions:\n  - match_prompt_contains: \"task-1\"\n    effects:\n      - { path: a1.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-1 }\n";

/// A run parked on its task's red criterion, with the attempt's check on the
/// log before anyone decides.
async fn parked_on_a_red_attempt() -> Bench {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(CHECKED_AS_IT_GOES, &missed_fixture()).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        sessions_and_checks(&bench.events(), "implement"),
        ["pre", "session", "post"],
        "the red attempt's check is on the log before anyone decides"
    );
    bench
}

/// Every check a task cycle runs reaches the log the moment it runs: the
/// pre-check before the task's first session opens, and its post-check
/// before a person decides — so the session a `retry` opens reads why the
/// attempt before it did not close.
#[tokio::test]
async fn a_red_attempts_check_reaches_the_log_before_anyone_decides() {
    let bench = parked_on_a_red_attempt().await;

    // The check is red on a tree that has not moved, so its menu offers
    // only abort: no retry is offered until the tree changes.
    assert_eq!(options(&bench).await, ["abort"]);

    // The person changes the tree; the wake that records the change offers
    // the retry.
    write(&bench.worktree.join("notes.txt"), "a note the person left");
    let RunReport { terminal, .. } = bench.wake().await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(options(&bench).await, ["retry", "abort"]);

    // A person retries; the session that opens meets the criterion.
    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, .. } = bench.wake_on_fixture(MADE_IT).await;
    assert_eq!(terminal, RunTerminal::Finished);
    // The log only grows: the first attempt's check stays ahead of it.
    let order = sessions_and_checks(&bench.events(), "implement");
    assert_eq!(
        order.iter().filter(|seen| **seen == "session").count(),
        2,
        "one session per attempt, the second one a person's: {order:?}"
    );
}

/// The coverage gate's own run counter, outside the tree so that counting
/// never changes what the gate judges.
fn counter(bench: &Bench) -> PathBuf {
    bench
        .worktree
        .parent()
        .expect("the worktree sits in the bench's world")
        .join("command-runs")
}

/// How many times the coverage command ran.
fn ran(bench: &Bench) -> usize {
    std::fs::read_to_string(counter(bench))
        .map(|runs| runs.lines().count())
        .unwrap_or(0)
}

/// A coverage gate that reads the percentage a person writes into `cov.txt`
/// and holds it to a threshold of 80.
fn coverage_config(bench: &Bench) -> String {
    format!(
        "{MOCK_CONFIG}coverage:\n  cmd: \"echo . >> {}; cat cov.txt\"\n  threshold: 80.0\n",
        counter(bench).display()
    )
}

/// One check node, which the coverage gate of the run's config measures.
const COVERAGE_WORKFLOW: &str =
    "name: covered\nnodes:\n  - { id: gate, kind: check, builtin: coverage_gate }\n";

/// The fixture of a workflow that opens no session.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// A run parked on its coverage gate, which measured 50% against 80.
async fn parked_on_coverage() -> Bench {
    parked_on_coverage_in(Bench::new()).await
}

/// [`parked_on_coverage`] in `bench`, which the caller shaped first: the
/// checkout a run starts on is the caller's to prepare.
async fn parked_on_coverage_in(bench: Bench) -> Bench {
    write(&bench.worktree.join("cov.txt"), "50%");
    let RunReport { terminal, .. } = bench
        .run_with_config(COVERAGE_WORKFLOW, NO_SESSIONS, &coverage_config(&bench))
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        options(&bench).await,
        ["abort"],
        "the unchanged tree offers abort alone"
    );
    assert_eq!(ran(&bench), 1, "the first attempt measured coverage once");
    bench
}

/// A coverage gate whose tree a person changed is offered a retry, and the
/// retry measures the tree again and finishes the run.
#[tokio::test]
async fn a_coverage_gate_retried_after_its_tree_moved_measures_again_and_finishes() {
    let bench = parked_on_coverage().await;

    write(&bench.worktree.join("cov.txt"), "90%");
    let RunReport { terminal, .. } = bench.wake().await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(options(&bench).await, ["retry", "abort"]);
    assert_eq!(
        ran(&bench),
        1,
        "a wake does not measure a failed gate again"
    );

    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, .. } = bench.wake().await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(ran(&bench), 2, "the retry measured the tree it moved to");
}

/// A retry answered for a tree that moved back to the one the gate failed on
/// runs nothing: the menu no longer offers it there, so the gate is not
/// measured and the run pauses on abort alone.
#[tokio::test]
async fn a_retry_answered_for_a_tree_moved_back_runs_no_gate() {
    let bench = parked_on_coverage().await;
    write(&bench.worktree.join("cov.txt"), "90%");
    bench.wake().await;
    assert_eq!(options(&bench).await, ["retry", "abort"]);
    answer_parked(&bench, "retry").await.unwrap();

    // The same bytes as at the failure, so the tree is the one it failed on.
    write(&bench.worktree.join("cov.txt"), "50%");
    let RunReport { terminal, .. } = bench.wake().await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(options(&bench).await, ["abort"]);
    assert_eq!(
        ran(&bench),
        1,
        "the answered retry measured nothing on the tree it failed on"
    );
}

/// The `.gitignore` a repository commits before its run: it names the one
/// file a person keeps under `.claude/`, a tree the project protects.
const IGNORES_LOCAL_SETTINGS: &str = ".claude/settings.local.json\n";

/// The tree `node`'s latest failure left the run on, and the tree the latest
/// wake recorded, as the log holds them.
fn failed_and_woken(bench: &Bench, node: &str) -> (Option<TreeId>, Option<TreeId>) {
    let events = bench.events();
    let failed = events.iter().rev().find_map(|event| {
        let of_node = event.node_id.as_ref().map(|id| id.as_str()) == Some(node);
        match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(failed))) if of_node => {
                Some(failed.tree.clone())
            }
            _ => None,
        }
    });
    let woken = events.iter().rev().find_map(|event| match event.payload() {
        Some(EventPayload::Run(RunEvent::Resumed(resumed))) => Some(resumed.tree.clone()),
        _ => None,
    });
    (failed.flatten(), woken.flatten())
}

/// A file git ignores under `.claude/` is not a move of the run's tree in a
/// run that commits its tree. The commit a node makes holds the files git
/// tracks or does not ignore, so the wake that finds the ignored file records
/// the tree the gate failed on, and the menu offers abort alone.
#[tokio::test]
async fn an_ignored_file_under_a_protected_tree_is_no_move_of_a_committing_run() {
    let bench = Bench::new();
    write(&bench.worktree.join(".gitignore"), IGNORES_LOCAL_SETTINGS);
    git(&bench.worktree, &["add", ".gitignore"]);
    git(
        &bench.worktree,
        &["commit", "-q", "-m", "ignore local settings"],
    );
    let bench = parked_on_coverage_in(bench).await;

    // The one change a person makes while the run is parked.
    write(&bench.worktree.join(".claude/settings.local.json"), "{}\n");
    let RunReport { terminal, .. } = bench.wake_on_fixture(NO_SESSIONS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let (failed, woken) = failed_and_woken(&bench, "gate");
    assert!(
        failed.is_some(),
        "the gate's failure names the tree it left"
    );
    assert_eq!(woken, failed, "the wake records the tree the failure left");
    assert_eq!(options(&bench).await, ["abort"]);
}

/// A fifo outside the tree, which the node parked beside a failing sibling
/// opens for reading and the test writes once.
fn release_fifo(bench: &Bench) -> PathBuf {
    let fifo = bench
        .worktree
        .parent()
        .expect("the worktree sits in the bench's world")
        .join("release");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "mkfifo {}", fifo.display());
    fifo
}

/// Writes the line the node parked on `release` reads, once the log shows
/// `node` failed: the sharer holds the run's tree until the failure has closed,
/// however long the failure takes.
async fn release_once_failed(bench: &Bench, node: &str, release: &Path) {
    yunta_testkit::wait_until_async(
        || std::future::ready(has_failed(bench, node)),
        || format!("node `{node}` never failed"),
    )
    .await;
    // The write waits for the node to open the fifo, so it runs on a thread of
    // its own and never holds the runtime the run is driven on.
    let release = release.to_path_buf();
    after(Duration::ZERO, move || release_now(&release));
}

/// Whether the log holds a failure of `node`.
fn has_failed(bench: &Bench, node: &str) -> bool {
    bench.events().iter().any(|event| {
        event.node_id.as_ref().map(|id| id.as_str()) == Some(node)
            && matches!(
                event.payload(),
                Some(EventPayload::Node(NodeEvent::Failed(_)))
            )
    })
}

/// Writes the line the node parked on the fifo reads.
fn release_now(release: &Path) {
    std::fs::write(release, "go\n").expect("the node parked on the fifo reads it");
}

/// Two top-level nodes that share the run's tree and run at once: `failing`
/// exits 1 while `beside` still holds the tree, waiting on `release`.
fn beside_failing(release: &Path) -> String {
    format!(
        "name: beside-failing\nnodes:\n  - id: failing\n    kind: bash\n    run: \"exit 1\"\n  - id: beside\n    kind: bash\n    run: \"read go < {}\"\n",
        release.display()
    )
}

/// A node that fails while another node shares the run's tree records the tree
/// its commit would hold, as a wake that finds nothing changed does, so the
/// failure and the wake read the same tree even with an ignored protected file.
#[tokio::test]
async fn a_failure_beside_a_sharer_records_the_tree_the_wake_finds() {
    let bench = Bench::new();
    write(&bench.worktree.join(".gitignore"), IGNORES_LOCAL_SETTINGS);
    git(&bench.worktree, &["add", ".gitignore"]);
    git(
        &bench.worktree,
        &["commit", "-q", "-m", "ignore local settings"],
    );
    write(&bench.worktree.join(".claude/settings.local.json"), "{}\n");
    let release = release_fifo(&bench);

    let config = format!("{MOCK_CONFIG}defaults:\n  max_parallel_nodes: 2\n");
    let workflow = beside_failing(&release);
    // The sharer is released once the failure has closed, so it still holds the
    // tree when the failure closes.
    let (RunReport { terminal, .. }, ()) = tokio::join!(
        bench.run_with_config(&workflow, NO_SESSIONS, &config),
        release_once_failed(&bench, "failing", &release),
    );
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let RunReport { terminal, .. } = bench.wake_on_fixture(NO_SESSIONS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let (failed, woken) = failed_and_woken(&bench, "failing");
    assert!(failed.is_some(), "the failure names the tree it left");
    assert_eq!(woken, failed, "the wake records the tree the failure left");
    assert_eq!(options(&bench).await, ["retry", "abort"]);
    assert_eq!(
        retries_wait_for(&bench).await,
        [Precondition::OutsideTreeChanged],
        "the tree did not move, so the retry waits on something outside it"
    );
}

/// A run that works in the tree it is given, with no isolation, names the
/// ignored protected file in the tree its gate failed on, so a wake that finds
/// nothing changed reads the same tree and offers abort alone.
#[tokio::test]
async fn an_ignored_file_under_a_protected_tree_is_no_move_of_a_run_working_in_place() {
    let bench = Bench::new();
    write(&bench.worktree.join(".gitignore"), IGNORES_LOCAL_SETTINGS);
    git(&bench.worktree, &["add", ".gitignore"]);
    git(
        &bench.worktree,
        &["commit", "-q", "-m", "ignore local settings"],
    );
    write(&bench.worktree.join(".claude/settings.local.json"), "{}\n");
    write(&bench.worktree.join("cov.txt"), "50%");
    let config = format!("{}defaults:\n  isolation: none\n", coverage_config(&bench));
    let RunReport { terminal, .. } = bench
        .run_with_config(COVERAGE_WORKFLOW, NO_SESSIONS, &config)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let RunReport { terminal, .. } = bench.wake_on_fixture(NO_SESSIONS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let (failed, woken) = failed_and_woken(&bench, "gate");
    assert!(
        failed.is_some(),
        "the gate's failure names the tree it left"
    );
    assert_eq!(woken, failed, "the wake records the tree the failure left");
    assert_eq!(options(&bench).await, ["abort"]);
}

/// A protected file a person writes while a run that works in place is parked
/// is a move of its tree: the close judges that tree with the file in it, so a
/// wake that finds the file reads the move and offers the retry.
#[tokio::test]
async fn a_protected_ignored_file_a_person_writes_is_a_move_of_a_run_working_in_place() {
    let bench = Bench::new();
    write(&bench.worktree.join(".gitignore"), IGNORES_LOCAL_SETTINGS);
    git(&bench.worktree, &["add", ".gitignore"]);
    git(
        &bench.worktree,
        &["commit", "-q", "-m", "ignore local settings"],
    );
    write(&bench.worktree.join("cov.txt"), "50%");
    let config = format!("{}defaults:\n  isolation: none\n", coverage_config(&bench));
    let RunReport { terminal, .. } = bench
        .run_with_config(COVERAGE_WORKFLOW, NO_SESSIONS, &config)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(options(&bench).await, ["abort"]);

    write(&bench.worktree.join(".claude/settings.local.json"), "{}\n");
    let RunReport { terminal, .. } = bench.wake_on_fixture(NO_SESSIONS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(options(&bench).await, ["retry", "abort"]);
    assert_eq!(
        retries_wait_for(&bench).await,
        [Precondition::TreeChanged],
        "the wake found the tree moved"
    );
}
