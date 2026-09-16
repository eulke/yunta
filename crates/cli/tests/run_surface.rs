//! What `yunta run` and `yunta resume` put in front of a person: the one
//! line `--quiet` leaves, the append-only lines a reader without a
//! terminal gets, and the block that closes a run out.
//!
//! Most commands here run with both streams piped, which is the
//! no-terminal case by construction — the same shape a CI log, a pipe
//! and a screen reader see. The pinned region exists only where there
//! is a terminal to pin it to, so what belongs to it is driven on a
//! pty instead.

use std::path::Path;

use yunta_testkit::{
    git, run_id_from, stderr, stdout, write, yunta_in, yunta_on_terminal, Checkout, Terminal,
};

/// A repo with `wf.yaml` written and committed, and the state root to run
/// it under. `isolation: none` keeps every node's work in this checkout,
/// which is what lets a test read what a run produced.
fn project(root: &Path, workflow: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let checkout = Checkout::under(root)
        .working_in_place()
        .workflow("wf", workflow)
        .committed();
    (checkout.repo, checkout.home)
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
        progress
            .lines()
            .any(|line| line.contains("touch — running") && line.starts_with('[')),
        "one line per moment, in the words a watched terminal uses, each \
         carrying the run's elapsed time: {progress}"
    );
}

#[test]
fn a_run_read_back_from_a_pipe_says_what_a_watched_terminal_kept() {
    // One chronicle, two layouts: a reader who followed the run on a
    // terminal and a reader who read it out of a pipe met the same
    // sentences. The terminal keeps what closed something; the pipe
    // writes every moment, that one included, word for word.
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), TWO_NODES);

    let piped = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(piped.status.success(), "{}", stderr(&piped));
    let written = stderr(&piped);

    let watched = tempfile::tempdir().unwrap();
    let (watched_repo, watched_home) = project(watched.path(), TWO_NODES);
    let mut terminal = yunta_on_terminal!(&watched_repo, &watched_home, &["run", "wf.yaml"]);
    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");

    // What a settled node left above the region is a sentence the pipe
    // wrote too — the elapsed column and the mark are layout, and the
    // words under them are the same words.
    for node in ["touch", "verify"] {
        let kept = format!("{node} — finished");
        assert!(
            drawn.contains(&kept),
            "the terminal kept `{kept}`:\n{drawn}"
        );
        assert!(written.contains(&kept), "and the pipe wrote it:\n{written}");
    }
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
fn a_resume_reports_what_it_did_and_not_what_the_log_already_held() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), TWO_NODES);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let run_id = run_id_from(&run);
    let first = stderr(&run).lines().filter(|l| l.starts_with('[')).count();
    assert!(
        first > 1,
        "the run reported its own events: {}",
        stderr(&run)
    );

    // The run already finished, so a resume has nothing to do and
    // nothing to say. Reporting the log it picked up would tell a
    // reader of a CI log that a finished run just ran again, and would
    // do it again on every resume after that.
    let resumed = yunta_in!(&repo, &home, &["resume", &run_id]);
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    let replayed: Vec<&str> = stderr(&resumed)
        .lines()
        .filter(|line| line.starts_with('['))
        .map(|line| line.to_string().leak() as &str)
        .collect();
    assert!(
        replayed.is_empty(),
        "the resume reported {} event(s) it did not produce: {replayed:?}",
        replayed.len()
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

/// A repo whose catalog holds the workflow `wf.yaml` composes, and the
/// state root to run it under. The child's one node runs `child_runs`,
/// which is what says how long the composition stays open. The child is
/// a run of its own, with its own id and its own log, so the parent is
/// left with the default isolation its children need.
fn composing(root: &Path, child_runs: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let checkout = Checkout::under(root)
        .file(
            ".yunta/workflows/child.yaml",
            &format!(
                "name: child\nnodes:\n  - {{ id: work, kind: bash, run: \"{child_runs}\" }}\n"
            ),
        )
        .workflow(
            "wf",
            "name: parent\nnodes:\n  - { id: compose, kind: workflow, use: child }\n",
        )
        .committed();
    (checkout.repo, checkout.home)
}

#[test]
fn the_live_region_shows_an_open_child_under_the_node_that_bore_it() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = composing(root.path(), "sleep 30");
    let mut terminal = yunta_on_terminal!(&repo, &home, &["run", "wf.yaml"]);

    terminal.wait_for(
        "child run ",
        "the region never drew the run this one composed",
    );
    let drawn = terminal.drawn();
    assert!(
        drawn.contains("still open"),
        "a child the parent's log has no close for is open and nothing more:\n{drawn}"
    );
    assert!(
        drawn.contains("compose"),
        "and it is read under the node that bore it:\n{drawn}"
    );

    terminal.interrupt();
    let drawn = terminal.ended();
    assert!(
        !terminal.ran_to_the_end(),
        "a run stopped by a person is not a success:\n{drawn}"
    );
}

#[test]
fn the_closing_block_shows_the_runs_this_one_composed_as_a_tree() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = composing(root.path(), "true");

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let text = stdout(&run);

    // Under the node that bore it, as a tree — never averaged into a
    // figure of the parent's own, which is what a percentage over
    // heterogeneous children would be.
    let children = text.find("children").expect("the composition block");
    let under = text
        .find("node `compose`")
        .expect("every child is read under the node that bore it");
    let child = text
        .find("· child run ")
        .expect("and named by the run id that is the only thing this log can name it by");
    assert!(children < under && under < child, "{text}");
    assert!(
        text.contains("finished · child run"),
        "carrying how the child closed, in the word its own run closed with: {text}"
    );
}

/// A node that holds the run open with nothing to ask anybody: the
/// region is on the terminal, and no prompt has taken it away.
const HOLDING: &str = r#"
name: holding
nodes:
  - id: hold
    kind: bash
    run: "sleep 30"
"#;

#[test]
fn a_diagnostic_raised_while_the_region_is_drawn_goes_out_above_it() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), HOLDING);
    let mut terminal = yunta_on_terminal!(&repo, &home, &["run", "wf.yaml"]);
    terminal.wait_for("nodes ", "the run never pinned its region to the terminal");

    terminal.interrupt();
    terminal.wait_for(
        "interrupt received",
        "the person who stopped the run was never told it is stopping",
    );

    // Nothing writes past the region: a line printed around it lands
    // inside the rows it is redrawing, and the next redraw erases the
    // copy it left there — so the confirmation a person just asked for
    // is on the terminal for under a second.
    assert!(
        terminal.cleared_before("nodes ", "interrupt received"),
        "the note landed inside the rows the region was redrawing:\n{}",
        terminal.drawn()
    );
    let drawn = terminal.ended();
    assert!(
        !terminal.ran_to_the_end(),
        "a run stopped by a person is not a success:\n{drawn}"
    );
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
        stdout(&loud).contains("past runs"),
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

#[test]
fn a_parked_run_is_called_the_same_thing_on_every_surface() {
    // One stop, five surfaces, one word. The vocabulary lives in
    // `render::state::RunWord`, and this is what holds every surface to
    // it: a reader who met `paused` on the block that closed the run
    // meets `paused` again on the status page, in both documents a
    // program reads, and in the listing's own grouping.
    const WORD: &str = "paused";

    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), EXHAUSTED);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&stdout(&run))
        .unwrap_or_else(|e| panic!("run --json emits JSON: {e}\n{}", stdout(&run)));
    let run_id = document["run_id"]
        .as_str()
        .expect("the run's id")
        .to_string();
    assert_eq!(document["outcome"], WORD, "run --json: {document:#}");

    let status = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    let from_status: serde_json::Value = serde_json::from_str(&stdout(&status))
        .unwrap_or_else(|e| panic!("status --json emits JSON: {e}"));
    assert_eq!(
        from_status["outcome"], WORD,
        "status --json: {from_status:#}"
    );

    let page = yunta_in!(&repo, &home, &["status", &run_id]);
    assert!(
        stdout(&page).contains(WORD),
        "the status page: {}",
        stdout(&page)
    );

    // Driven again with no flag at all, so the block that closes a run
    // out says it in the words a person reads.
    let closing = yunta_in!(&repo, &home, &["resume", &run_id]);
    assert!(
        stdout(&closing).contains(WORD),
        "the closing block: {}",
        stdout(&closing)
    );

    // The listing groups by the same word, so the heading over a run and
    // the word for that run cannot disagree.
    let listing = yunta_in!(&repo, &home, &["list", "--runs"]);
    let text = stdout(&listing);
    let needs_you = text
        .find("needs you")
        .unwrap_or_else(|| panic!("a group for it: {text}"));
    let listed = text
        .find(&run_id)
        .unwrap_or_else(|| panic!("the run itself: {text}"));
    assert!(needs_you < listed, "the run is under that heading: {text}");
}

#[test]
fn a_run_reports_the_same_word_on_a_terminal_as_off_one() {
    // The surface a person actually watches is drawn on a pty and takes
    // a different path to the same block: a live terminal puts the menu
    // to the person instead of parking on it, so the run stops here
    // because they walked away from it. It ends in the same word.
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), EXHAUSTED);

    let mut terminal = yunta_on_terminal!(&repo, &home, &["run", "wf.yaml"]);
    terminal.wait_for("esc parks the run", "the menu was never put to anyone");
    terminal.keys("\x1b");

    let drawn = terminal.ended();
    assert!(
        !terminal.ran_to_the_end(),
        "a parked run is not a success:\n{drawn}"
    );
    assert!(drawn.contains("paused"), "{drawn}");
}

/// A node that reviews and finds something blocking. The run itself
/// reaches its end: the work is done, and nobody has accepted it.
const REVIEWS: &str = r#"
name: reviewed
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the change."
    artifacts:
      produces: [findings]
"#;

const FOUND_SOMETHING_BLOCKING: &str = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: null-deref
          severity: blocking
          title: "Resize handler dereferences a null pointer"
          location: "src/ui/resize.rs:142"
          detail: "Resizing before the first paint reaches a null surface."
    outcome: { type: completed, summary: "reviewed" }
"#;

#[test]
fn a_run_that_finished_holding_blocking_findings_is_not_a_success() {
    // One mapping answers "did the command succeed", and it answers no
    // here: the work ran to its end and nobody has accepted it, which
    // is what the block above the exit code says in words. Every
    // surface reports it the same way, because each reads the same two
    // facts off the same log.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let home = root.path().join("state");
    write(
        &repo.join(".yunta/config.yaml"),
        "runners:\n  executor:\n    - { adapter: claude-code, model: claude-model }\n",
    );
    write(&repo.join("wf.yaml"), REVIEWS);
    write(&repo.join("fixture.yaml"), FOUND_SOMETHING_BLOCKING);
    yunta_testkit::init_repo(&repo);

    let mocked = ["--adapter", "mock", "--fixture", "fixture.yaml"];
    let run = yunta_in!(
        &repo,
        &home,
        &[&["run", "wf.yaml"][..], &mocked[..]].concat()
    );
    let text = stdout(&run);
    assert!(
        text.contains("finished, holding 1 blocking finding"),
        "the block says what is holding it: {text}\nstderr: {}",
        stderr(&run)
    );
    assert!(
        !run.status.success(),
        "and the exit code says the same thing: {text}"
    );
    let run_id = run_id_from(&run);

    // The document a program reads carries the count, so a reader with
    // only this reaches the same conclusion.
    let status = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    let document: serde_json::Value = serde_json::from_str(&stdout(&status))
        .unwrap_or_else(|e| panic!("status --json emits JSON: {e}"));
    assert_eq!(document["outcome"], "finished", "{document:#}");
    assert_eq!(document["blocking_findings"], 1, "{document:#}");
}

/// A `parallel` group holding the run open with its two children
/// working: the region is on the terminal, drawn from the frame.
const GROUPED: &str = r#"
name: grouped
nodes:
  - id: sweep
    kind: parallel
    nodes:
      - { id: sweep-a, kind: bash, scope: ["a/**"], run: "sleep 30" }
      - { id: sweep-b, kind: bash, scope: ["b/**"], run: "sleep 30" }
"#;

/// The last painting that carried every one of `headlines`, and `None`
/// while no painting has carried them all.
///
/// A painting goes out as one run of text between escape sequences, each
/// row padded to the terminal's full width, so cutting that run into
/// widths gives back the rows a reader was looking at — leading spaces
/// and all.
fn painting_with(drawn: &str, headlines: &[String]) -> Option<Vec<String>> {
    let painting = drawn
        .split('\u{1b}')
        .filter_map(|chunk| chunk.find(char::is_alphabetic).map(|end| &chunk[end + 1..]))
        .rfind(|text| headlines.iter().all(|headline| text.contains(headline)))?;
    Some(
        painting
            .chars()
            .collect::<Vec<char>>()
            .chunks(usize::from(Terminal::COLUMNS))
            .map(|row| row.iter().collect())
            .collect(),
    )
}

#[test]
fn the_live_view_indents_a_groups_children_under_it() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), GROUPED);
    let mut terminal = yunta_on_terminal!(&repo, &home, &["run", "wf.yaml"]);

    // One painting carrying all three, not three paintings carrying one
    // each: the region draws a node once it is working, the two children
    // start in whichever order the scheduler reaches them, and whichever
    // starts first is drawn in a painting the other is missing from.
    // What this asserts on is where the rows sit relative to each other,
    // which only a painting that holds them all can answer.
    //
    // A row is found by the node it is about and not by the mark in
    // front of it: that mark says how the node is doing and changes
    // while it works, and what this is about is the indentation, which
    // sits before the mark either way. The trailing separator is part of
    // the needle because `sweep` is a prefix of `sweep-a`.
    let headline = |id: &str| format!("run {id} ·");
    let wanted = ["sweep", "sweep-a", "sweep-b"].map(headline);
    let rows = yunta_testkit::wait_for(
        || painting_with(&terminal.drawn(), &wanted),
        || {
            format!(
                "the region never drew one painting carrying {wanted:?}\ndrawn so far:\n{}",
                terminal.drawn()
            )
        },
    );
    let depth = |id: &str| {
        let row = rows
            .iter()
            .find(|row| row.contains(&headline(id)))
            .unwrap_or_else(|| panic!("`{id}` has a row of its own:\n{rows:#?}"));
        row.len() - row.trim_start().len()
    };

    assert!(
        depth("sweep-a") > depth("sweep"),
        "a group's children sit under the group:\n{rows:#?}"
    );
    assert_eq!(
        depth("sweep-a"),
        depth("sweep-b"),
        "and level with each other:\n{rows:#?}"
    );

    terminal.interrupt();
    assert!(
        !terminal.ran_to_the_end(),
        "a run stopped by a person is not a success:\n{}",
        terminal.ended()
    );
}
