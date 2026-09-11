//! A run parked on a person, from both sides a person meets it:
//! `yunta list --runs` finds it without reading every line, and `yunta
//! status` answers it without guessing an option id.
//!
//! Both surfaces are driven through the real compiled binary, the same
//! way `run_flow.rs` drives a run to a gate, because what is being
//! checked is what a second terminal sees — a process that never started
//! the run and has nothing but its log to read.

use std::path::{Path, PathBuf};

use yunta_testkit::{git, init_repo, run_id_from, stdout, wait_for, write, yunta_in, MOCK_CONFIG};

/// A node that fails with its one re-route already spent: the run parks
/// on a decision whose menu is rebuilt from the log alone.
const EXHAUSTED_REROUTE: &str = r#"
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

/// A gate nobody has answered yet: the run parks on the menu its own
/// manifest declares, and the engine attaches who the gate is assigned
/// to — which the message it asks with never says.
const INTERNAL_GATE: &str = r#"
name: approval
nodes:
  - id: plan
    kind: bash
    run: "true"
  - id: approve
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Approve the plan?"
    options: [ship, adjust]
    on: { adjust: plan }
"#;

/// A node that fails with no `on_failure` at all: the run parks with
/// nothing to choose from — one of the pauses no menu reconstructs.
const PLAIN_FAILURE: &str = r#"
name: plain-failure
nodes:
  - id: boom
    kind: bash
    run: "exit 3"
"#;

/// A node whose session writes a questions artifact and ends: nothing
/// answers the questions with stdin closed, so the run parks with the
/// node itself waiting on a person rather than on a failure.
const ASKING: &str = r#"
name: asking
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: "Ask what has to be known before going on."
    artifacts:
      produces:
        - { name: questions.yaml, kind: questions }
"#;

/// The scripted session behind [`ASKING`]: it writes the artifact and
/// closes, so the round with a person is reached with no agent
/// installed.
const ASKING_FIXTURE: &str = r#"
sessions:
  - effects:
      - path: "{{run.dir}}/artifacts/questions.yaml"
        content: |
          questions:
            - id: summary
              text: "What changed?"
              answer_type: text
              required: true
    outcome: { type: completed, summary: asked }
"#;

/// A node that holds the run open until the test hands it `go.txt`. It
/// announces itself with `started.txt` first, so the test watches for a
/// marker the node writes instead of waiting a fixed time for it.
const HOLDS_OPEN: &str = r#"
name: holds-open
nodes:
  - id: hold
    kind: bash
    run: "touch started.txt; until [ -f go.txt ]; do :; done"
"#;

/// The width a rendered line stays inside, and the width a reader's
/// terminal is taken to have.
const LINE_WIDTH: usize = 80;

/// A git repo carrying `workflows` as `<name>.yaml`, committed — every
/// run starts from a clean tree.
fn repo_with(root: &Path, workflows: &[(&str, &str)]) -> PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).expect("create the repo directory");
    init_repo(&repo);
    for (name, contents) in workflows {
        write(&repo.join(format!("{name}.yaml")), contents);
    }
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "workflows"]);
    repo
}

/// The lines of the `decision needed` block: everything from its heading
/// to the end of the page.
fn decision_block(text: &str) -> Vec<&str> {
    text.lines()
        .skip_while(|line| !line.starts_with("decision needed"))
        .collect()
}

/// The decision the closing block carries: its `waiting on node`
/// heading and every line under it, stopping at the labelled rows that
/// report where the run lived.
fn closing_decision(text: &str) -> Vec<&str> {
    text.lines()
        .skip_while(|line| !line.trim_start().starts_with("waiting on node"))
        .take_while(|line| !line.trim_start().starts_with("progress "))
        .collect()
}

#[test]
fn status_of_a_parked_run_shows_every_option_and_the_command_that_answers_it() {
    let root = tempfile::tempdir().unwrap();
    let repo = repo_with(root.path(), &[("hopeless", EXHAUSTED_REROUTE)]);
    let home = root.path().join("state");

    let run = yunta_in!(&repo, &home, &["run", "hopeless.yaml"]);
    let run_id = run_id_from(&run);
    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    assert!(
        status.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let text = stdout(&status);

    assert!(
        text.contains("decision needed on node `lint`"),
        "the page says which node the decision belongs to: {text}"
    );
    // The option ids a person types, each with the tradeoff that makes
    // choosing one a decision rather than a guess.
    assert!(
        text.contains("retry — Re-route to `fix-lint` once more"),
        "{text}"
    );
    assert!(text.contains("abort — Abort the run"), "{text}");
    assert_eq!(
        text.matches("tradeoff:").count(),
        2,
        "one tradeoff per option: {text}"
    );

    // The command carries the run's own id and leaves the option open:
    // an example option is what gets pasted.
    assert!(
        text.contains(&format!("yunta resolve-gate {run_id} <option>")),
        "{text}"
    );
    assert!(
        !text.contains(&format!("yunta resolve-gate {run_id} retry")),
        "no option is offered as a command to paste: {text}"
    );

    for line in decision_block(&text) {
        assert!(
            line.chars().count() <= LINE_WIDTH,
            "line exceeds {LINE_WIDTH} columns ({} chars): {line:?}",
            line.chars().count()
        );
        assert!(
            !line.contains('\x1b'),
            "line uses an ANSI escape code, not colorless: {line:?}"
        );
    }
}

#[test]
fn one_decision_reads_as_a_trailer_when_a_run_stops_and_as_a_page_when_it_is_asked_about() {
    // The block a run leaves on the terminal and the page `yunta status`
    // prints carry the same escalation: neither drops a part the other
    // keeps. What differs is the room each part is given and the
    // headings a page has space for.
    let root = tempfile::tempdir().unwrap();
    let repo = repo_with(root.path(), &[("hopeless", EXHAUSTED_REROUTE)]);
    let home = root.path().join("state");

    let run = yunta_in!(&repo, &home, &["run", "hopeless.yaml"]);
    let run_id = run_id_from(&run);
    let closing = stdout(&run);
    let trailer = closing_decision(&closing);
    let page_text = stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    let page = decision_block(&page_text);

    for block in [&trailer, &page] {
        let text = block.join("\n");
        assert!(text.contains("node `lint`"), "{text}");
        assert!(
            text.contains("retry — Re-route to `fix-lint` once more"),
            "{text}"
        );
        assert!(text.contains("abort — Abort the run"), "{text}");
        assert_eq!(
            text.matches("tradeoff:").count(),
            2,
            "one tradeoff per option: {text}"
        );
        assert!(
            text.contains(&format!("yunta resolve-gate {run_id} <option>")),
            "{text}"
        );
    }

    // A trailer hangs under the outcome above it: each part keeps its
    // label inline on the one line it gets, and the last word is that
    // nothing has to stay open for the answer.
    assert_eq!(
        trailer.first().copied(),
        Some("  waiting on node `lint`"),
        "{trailer:?}"
    );
    for heading in ["evidence:", "options:", "answer it with:"] {
        assert!(
            !trailer.iter().any(|line| line.trim() == heading),
            "a trailer has no headings of its own: {trailer:?}"
        );
    }
    assert!(
        trailer
            .last()
            .is_some_and(|line| line.contains("close this terminal whenever you like")),
        "{trailer:?}"
    );

    // A page opens on the decision and hangs every part under its own
    // heading. It carries no aside: nobody is waiting in front of it.
    assert_eq!(
        page.first().copied(),
        Some("decision needed on node `lint`:"),
        "{page:?}"
    );
    for heading in ["  options:", "  answer it with:"] {
        assert!(page.contains(&heading), "{page:?}");
    }
    assert!(!page_text.contains("close this terminal"), "{page_text}");
}

#[test]
fn evidence_is_printed_only_where_it_says_more_than_the_summary_already_did() {
    let root = tempfile::tempdir().unwrap();
    let repo = repo_with(
        root.path(),
        &[("hopeless", EXHAUSTED_REROUTE), ("approval", INTERNAL_GATE)],
    );
    let home = root.path().join("state");

    // An exhausted re-route: the engine quotes the failure into the
    // summary and attaches that same failure as the evidence under it.
    // A reader meets the failure once, and no heading promises a record
    // that turns out to be the sentence above it.
    let run = yunta_in!(&repo, &home, &["run", "hopeless.yaml"]);
    let run_id = run_id_from(&run);
    let closing = stdout(&run);
    let status = stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    for block in [
        closing_decision(&closing).join("\n"),
        decision_block(&status).join("\n"),
    ] {
        assert!(
            !block.contains("evidence"),
            "the summary already carries it: {block}"
        );
        assert_eq!(
            block.matches("exit 1").count(),
            1,
            "the failure the run stopped on is named once: {block}"
        );
    }

    // A gate: the message it asks with never says who is being asked, so
    // the evidence is a part of its own — inline on the trailer, under
    // its own heading on the page.
    let gate = yunta_in!(&repo, &home, &["run", "approval.yaml"]);
    let gate_id = run_id_from(&gate);
    let gate_closing = stdout(&gate);
    let gate_status = stdout(&yunta_in!(&repo, &home, &["status", &gate_id]));
    let trailer = closing_decision(&gate_closing).join("\n");
    let page = decision_block(&gate_status).join("\n");
    assert!(trailer.contains("evidence: assignee: lead"), "{trailer}");
    assert!(
        page.contains("  evidence:") && page.contains("assignee: lead"),
        "{page}"
    );
}

#[test]
fn the_menu_a_person_reads_is_the_menu_a_program_reads() {
    let root = tempfile::tempdir().unwrap();
    let repo = repo_with(root.path(), &[("hopeless", EXHAUSTED_REROUTE)]);
    let home = root.path().join("state");

    let run = yunta_in!(&repo, &home, &["run", "hopeless.yaml"]);
    let run_id = run_id_from(&run);
    let status = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    let state: serde_json::Value = serde_json::from_slice(&status.stdout)
        .unwrap_or_else(|e| panic!("status --json emits JSON: {e}\n{}", stdout(&status)));

    let decision = &state["decision"];
    assert_eq!(decision["node"], "lint", "{state:#}");
    let options = decision["options"]
        .as_array()
        .unwrap_or_else(|| panic!("the menu is data, not a sentence: {state:#}"));
    let ids: Vec<&str> = options.iter().filter_map(|o| o["id"].as_str()).collect();
    assert_eq!(ids, ["retry", "abort"], "{state:#}");
    assert!(
        options
            .iter()
            .all(|option| !option["tradeoff"].as_str().unwrap_or_default().is_empty()),
        "every option carries its tradeoff: {state:#}"
    );
    assert_eq!(
        decision["resolve_with"],
        serde_json::Value::String(format!("yunta resolve-gate {run_id} <option>")),
        "{state:#}"
    );
    // What a page drops because the summary already said it, a document
    // still carries: the machine surface is the escalation as the log
    // recorded it, never what a reader was shown.
    assert!(
        decision["evidence"]
            .as_str()
            .is_some_and(|text| text.contains("exit 1")),
        "{state:#}"
    );

    // Additive: the document a reader already parses is untouched —
    // same version, same fields, one more of them.
    assert_eq!(state["schema_version"], 2, "{state:#}");
    assert!(
        state["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("waiting — node `lint` failed"),
        "{state:#}"
    );
    assert!(state["nodes"]["lint"].is_string(), "{state:#}");
}

#[test]
fn a_pause_with_no_menu_still_says_what_the_run_is_waiting_on() {
    // Two shapes reconstruct a menu — an exhausted re-route and an
    // unresolved internal gate. A plain failure is neither, and the page
    // says so rather than leaving the reader to wonder where the options
    // went.
    let root = tempfile::tempdir().unwrap();
    let repo = repo_with(root.path(), &[("plain", PLAIN_FAILURE)]);
    let home = root.path().join("state");

    let run = yunta_in!(&repo, &home, &["run", "plain.yaml"]);
    let run_id = run_id_from(&run);
    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    let text = stdout(&status);

    assert!(text.contains("waiting on:"), "{text}");
    assert!(
        text.contains("node `boom` failed"),
        "the reason it stopped is on the page: {text}"
    );
    assert!(
        text.contains("no options to choose"),
        "the absent menu is named, not silently missing: {text}"
    );
    assert!(
        text.contains(&format!("yunta resume {run_id}")),
        "the way back into the run is on the page: {text}"
    );
    assert!(
        !text.contains("resolve-gate"),
        "nothing offers a command that cannot answer this pause: {text}"
    );

    let status_json = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    let state: serde_json::Value = serde_json::from_slice(&status_json.stdout)
        .unwrap_or_else(|e| panic!("status --json emits JSON: {e}"));
    assert!(
        state.get("decision").is_none(),
        "a pause with no menu carries none: {state:#}"
    );
}

#[test]
fn the_listing_puts_a_waiting_run_above_a_running_one() {
    let root = tempfile::tempdir().unwrap();
    let repo = repo_with(
        root.path(),
        &[("holds-open", HOLDS_OPEN), ("hopeless", EXHAUSTED_REROUTE)],
    );
    let home = root.path().join("state");

    // A run that is still going: its node holds the worktree open until
    // this test hands it `go.txt`.
    let mut running = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "holds-open.yaml"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to run the yunta binary");
    let worktree = wait_for(
        || {
            let entry = std::fs::read_dir(home.join("worktrees"))
                .ok()?
                .flatten()
                .next()?;
            entry
                .path()
                .join("started.txt")
                .exists()
                .then(|| entry.path())
        },
        || "the holding node never started".to_string(),
    );

    // And a run that stopped on a person, started after it.
    let parked = yunta_in!(&repo, &home, &["run", "hopeless.yaml"]);
    let parked_id = run_id_from(&parked);

    let list = yunta_in!(&repo, &home, &["list", "--runs"]);
    assert!(
        list.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let text = stdout(&list);

    let needs_you = position(&text, "needs you (1)");
    let in_flight = position(&text, "in flight (1)");
    assert!(
        needs_you < in_flight,
        "what waits on a person comes first: {text}"
    );
    assert!(
        position(&text, &parked_id) < in_flight,
        "the parked run is in the group that needs a person: {text}"
    );
    assert!(
        text.contains("hopeless (default)"),
        "a row names the workflow and the mode, not only the id: {text}"
    );
    assert!(
        text.contains("waiting — node `lint` failed"),
        "the row says what the wait is for, in words: {text}"
    );
    for line in text.lines() {
        assert!(
            line.chars().count() <= LINE_WIDTH,
            "line exceeds {LINE_WIDTH} columns ({} chars): {line:?}",
            line.chars().count()
        );
        assert!(
            !line.contains('\x1b'),
            "line uses an ANSI escape code, not colorless: {line:?}"
        );
    }

    write(&worktree.join("go.txt"), "go");
    let finished = running.wait().expect("the running run exits");
    assert!(finished.success());
}

#[test]
fn a_run_parked_on_a_node_names_the_node_and_what_that_node_asked_for() {
    // A node parked on a person is the phase every surface reports, so
    // the one-line summary names the node. The page under it has room
    // for the sentence the engine wrote when it stopped the run, and
    // that sentence — not the node id — is what says which questions are
    // still unanswered.
    let root = tempfile::tempdir().unwrap();
    let repo = repo_with(
        root.path(),
        &[("asking", ASKING), ("fixture", ASKING_FIXTURE)],
    );
    let home = root.path().join("state");
    write(&repo.join(".yunta/config.yaml"), MOCK_CONFIG);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "runners"]);

    let run = yunta_in!(
        &repo,
        &home,
        &[
            "run",
            "asking.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml"
        ]
    );
    let run_id = run_id_from(&run);

    let text = stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    assert!(
        text.contains("1 waiting · 0 reroutes · waiting — node `ask`"),
        "the summary counts the parked node and names it: {text}"
    );
    assert!(
        text.contains("node `ask` asked 1 question(s) awaiting an answer: summary"),
        "the page names the question still unanswered, not only the node: {text}"
    );
    assert!(
        text.contains(&format!("yunta resume {run_id}")),
        "the way back into the run is on the page: {text}"
    );

    let list = stdout(&yunta_in!(&repo, &home, &["list", "--runs"]));
    assert!(
        list.contains("needs you (1)") && list.contains("waiting — node `ask`"),
        "the listing groups it with what needs a person, under the same words: {list}"
    );
}

/// Where `needle` starts in `text`, failing with the text itself when it
/// is not there at all.
fn position(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is missing from:\n{text}"))
}

#[test]
fn a_run_whose_manifest_does_not_read_back_is_listed_as_itself() {
    let root = tempfile::tempdir().unwrap();
    let repo = repo_with(root.path(), &[("plain", PLAIN_FAILURE)]);
    let home = root.path().join("state");

    let run = yunta_in!(&repo, &home, &["run", "plain.yaml"]);
    let run_id = run_id_from(&run);
    std::fs::remove_file(home.join("runs").join(&run_id).join("manifest.yaml"))
        .expect("the run's manifest is there to remove");

    let list = yunta_in!(&repo, &home, &["list", "--runs"]);
    assert!(list.status.success());
    let text = stdout(&list);
    assert!(text.contains("unreadable (1)"), "{text}");
    assert!(
        text.contains(&run_id),
        "a run that cannot be derived is still named: {text}"
    );
}
