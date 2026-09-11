//! What `yunta run` and `yunta resume` put in front of a person: the one
//! line `--quiet` leaves, the append-only lines a reader without a
//! terminal gets, and the block that closes a run out.
//!
//! Every command here runs with both streams piped, which is the
//! no-terminal case by construction — the same shape a CI log, a pipe
//! and a screen reader see.

use std::path::Path;

use yunta_testkit::{git, init_repo, run_id_from, stderr, stdout, write, yunta_in};

/// A repo with `wf.yaml` written and committed, and the state root to run
/// it under. `isolation: none` keeps every node's work in this checkout,
/// which is what lets a test read what a run produced.
fn project(root: &Path, workflow: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(&repo.join("wf.yaml"), workflow);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);
    (repo, root.join("state"))
}

const TWO_NODES: &str = r#"
name: two-nodes
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
  - id: verify
    kind: bash
    depends_on: [touch]
    run: "test -f made.txt"
"#;

/// A node whose re-routes are exhausted the moment it fails: the run
/// parks on a decision whose menu is rebuildable from the log alone.
const EXHAUSTED: &str = r#"
name: hopeless
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "touch fixed.txt"
"#;

#[test]
fn quiet_prints_one_line_and_lets_the_exit_code_carry_the_verdict() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), TWO_NODES);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--quiet"]);
    assert!(run.status.success(), "{}", stderr(&run));

    let text = stdout(&run);
    assert_eq!(
        text.lines().count(),
        1,
        "quiet is one line and nothing else, got: {text:?}"
    );
    let run_id = run_id_from(&run);
    assert!(
        text.starts_with(&format!("run {run_id}:")),
        "the line every run-based test reads the id from: {text:?}"
    );
    assert_eq!(
        stderr(&run),
        "",
        "quiet has no progress to announce standing down"
    );
}

#[test]
fn quiet_still_reports_a_failing_verdict_through_the_exit_code_alone() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), EXHAUSTED);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--quiet"]);
    assert!(
        !run.status.success(),
        "a parked run is not a success: {}",
        stdout(&run)
    );
    assert_eq!(stdout(&run).lines().count(), 1, "{}", stdout(&run));
}

#[test]
fn resume_is_quiet_on_the_same_terms_run_is() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), TWO_NODES);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--quiet"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let run_id = run_id_from(&run);

    let resumed = yunta_in!(&repo, &home, &["resume", &run_id, "--quiet"]);
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    assert_eq!(
        stdout(&resumed).lines().count(),
        1,
        "one line from either command, got: {:?}",
        stdout(&resumed)
    );
    assert!(
        stdout(&resumed).starts_with(&format!("run {run_id}:")),
        "carrying the run id: {:?}",
        stdout(&resumed)
    );
    assert_eq!(
        stderr(&resumed),
        "",
        "and no progress announcing that it stood down"
    );
}

#[test]
fn without_a_terminal_line_one_names_the_downgrade_and_nothing_redraws() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), TWO_NODES);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "{}", stderr(&run));

    let progress = stderr(&run);
    assert_eq!(
        progress.lines().next(),
        Some("live view off (stderr is not a terminal): one line per event"),
        "got: {progress}"
    );
    assert!(
        !progress.contains('\u{1b}') && !progress.contains('\r'),
        "nothing redraws where there is nothing to redraw on: {progress:?}"
    );
    assert!(
        progress.lines().any(|line| line.contains("node_started")
            && line.contains("`touch`")
            && line.starts_with('[')),
        "one line per event, each carrying the run's elapsed time: {progress}"
    );
}

#[test]
fn the_closing_block_names_a_finished_run_and_what_to_do_with_it() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), TWO_NODES);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let run_id = run_id_from(&run);
    let text = stdout(&run);

    assert!(
        text.contains(&format!("run {run_id}: ")) && text.contains("finished"),
        "the outcome comes first, as a word: {text}"
    );
    for label in [
        "progress",
        "tokens",
        "slowest",
        "branch",
        "artifacts",
        "next",
    ] {
        assert!(text.contains(label), "no `{label}` row in: {text}");
    }
    assert!(
        text.contains(&format!("yunta receipt {run_id}")),
        "a finished run's next command certifies it: {text}"
    );
    assert!(
        text.contains("nodes 2/2"),
        "counters, never a percentage: {text}"
    );
}

#[test]
fn the_closing_block_leads_with_the_decision_a_parked_run_waits_on() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), EXHAUSTED);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success(), "{}", stdout(&run));
    let run_id = run_id_from(&run);
    let text = stdout(&run);

    assert!(text.contains("paused"), "the outcome, as a word: {text}");
    assert!(
        text.contains("waiting on node `lint`"),
        "what it waits on: {text}"
    );
    assert!(
        text.matches("tradeoff:").count() >= 2,
        "every option carries the tradeoff that makes it a choice: {text}"
    );
    assert!(
        text.contains(&format!("yunta resolve-gate {run_id} <option>")),
        "the exact command, with the option left to the reader: {text}"
    );
    assert!(
        text.contains("close this terminal whenever you like"),
        "the run holds its own state, and says so: {text}"
    );

    // The decision is above the counters: the most actionable thing is
    // the most prominent thing.
    let decision = text.find("waiting on node").expect("the decision block");
    let counters = text.find("progress ").expect("the counters row");
    assert!(decision < counters, "{text}");
}

#[test]
fn resume_reports_exactly_what_run_reports() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), EXHAUSTED);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    let run_id = run_id_from(&run);

    // Handed back untouched, the run parks on the same decision — and
    // says so in the same block, from the other command.
    let resumed = yunta_in!(&repo, &home, &["resume", &run_id]);
    let text = stdout(&resumed);
    assert!(
        text.contains("paused on a decision") && text.contains("waiting on node `lint`"),
        "{text}"
    );
    for label in ["progress", "tokens", "branch", "artifacts", "next"] {
        assert!(text.contains(label), "no `{label}` row in: {text}");
    }
    assert_eq!(
        stderr(&resumed).lines().next(),
        Some("live view off (stderr is not a terminal): one line per event"),
        "resume degrades and announces it exactly as run does: {}",
        stderr(&resumed)
    );
}

#[test]
fn resume_json_is_the_document_run_json_prints() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), TWO_NODES);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--json"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let from_run: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let run_id = from_run["run_id"].as_str().unwrap().to_string();

    let resumed = yunta_in!(&repo, &home, &["resume", &run_id, "--json"]);
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    let from_resume: serde_json::Value = serde_json::from_slice(&resumed.stdout).unwrap();

    assert_eq!(from_resume["schema_version"], from_run["schema_version"]);
    assert_eq!(from_resume["run_id"], from_run["run_id"]);
    assert_eq!(from_resume["outcome"], "finished");
}

/// A prompt node on the mock adapter, whose session reports the usage
/// that gives this workflow a token history to compare a budget against.
const SPENDING: &str = r#"
name: spending
nodes:
  - id: think
    kind: prompt
    runner: planner
    prompt: "think"
"#;

const SPENDING_FIXTURE: &str = r#"
capabilities: { usage_reporting: true }
steps:
  - { type: usage, input_tokens: 400, output_tokens: 100 }
outcome: { type: completed, summary: "thought it through" }
"#;

#[test]
fn the_budget_warning_survives_quiet_and_the_distribution_does_not() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), SPENDING);
    write(&repo.join("fixture.yaml"), SPENDING_FIXTURE);
    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
         - { adapter: mock, model: mock-model }\n",
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "runners"]);

    let mocked = [
        "run",
        "wf.yaml",
        "--adapter",
        "mock",
        "--fixture",
        "fixture.yaml",
    ];
    // The estimation stays silent below its own three-run floor, so three
    // runs are what earn this workflow a distribution at all.
    for _ in 0..3 {
        let run = yunta_in!(&repo, &home, &mocked);
        assert!(run.status.success(), "{}", stderr(&run));
    }

    // A budget under what this workflow has historically spent.
    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\nlimits:\n  max_tokens_per_run: 1\nrunners:\n  planner:\n    \
         - { adapter: mock, model: mock-model }\n",
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "budget"]);

    let loud = yunta_in!(&repo, &home, &mocked);
    assert!(
        stdout(&loud).contains("past run(s)"),
        "the distribution is shown when nobody asked for silence: {}",
        stdout(&loud)
    );

    let mut quiet: Vec<&str> = mocked.to_vec();
    quiet.push("--quiet");
    let quiet = yunta_in!(&repo, &home, &quiet);
    assert_eq!(
        stdout(&quiet).lines().count(),
        1,
        "the distribution is context nobody asked for: {}",
        stdout(&quiet)
    );
    assert!(
        stderr(&quiet).contains("max_tokens_per_run") && stderr(&quiet).contains("p90"),
        "the warning asks for a decision before anything is spent, so it survives: {}",
        stderr(&quiet)
    );
}
