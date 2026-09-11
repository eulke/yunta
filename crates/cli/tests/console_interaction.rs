//! What a person meets when a run stops and asks them something, driven
//! end to end: a real `yunta run` with a terminal on both ends of its
//! stdio, so the console surface engages instead of degrading to a
//! pause, and the test types the keys a person would.
//!
//! A terminal is the only place these behaviors exist. A pasted line
//! break is the byte a typed Enter is; an arrow is an escape sequence
//! and not a character; Escape is a decline and Ctrl-C is not; and the
//! read that waits for all of them has to stay off the run's own
//! runtime, where the Ctrl-C handler and the per-node run-tools
//! listener are waiting too.

use std::path::Path;

use yunta_core::text::indent;
use yunta_testkit::{git, init_repo, runs_root, wait_until, write, yunta_on_terminal, Terminal};

/// A node that asks: the session writes the questions artifact and
/// ends, and the round with the person happens after it closes.
const ASKING: &str = "\
name: asking
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: \"Ask what has to be known before going on.\"
    artifacts:
      produces:
        - { name: questions.yaml, kind: questions }
";

/// The session that writes the artifact, scripted — the round a person
/// answers is reached with no agent installed.
const WROTE_THEM: &str = r#"
sessions:
  - effects:
      - path: "{{run.dir}}/artifacts/questions.yaml"
        content: |
QUESTIONS
    outcome: { type: completed, summary: asked }
"#;

const CONFIG: &str = "\
defaults:
  isolation: none
runners:
  executor:
    - { adapter: mock, model: mock-model }
";

/// The state root a run started under `root` keeps its runs in — the
/// same directory [`asking`] and [`gated`] hand the run.
fn home(root: &Path) -> std::path::PathBuf {
    root.join("state")
}

/// The answers artifact the run under `root` recorded, or `None` while
/// it has recorded none.
fn answers(root: &Path) -> Option<String> {
    let runs = std::fs::read_dir(runs_root(&home(root))).ok()?;
    runs.flatten()
        .flat_map(|run| std::fs::read_dir(run.path().join("artifacts")))
        .flatten()
        .flatten()
        .find(|artifact| {
            artifact
                .file_name()
                .to_string_lossy()
                .ends_with(".answers.yaml")
        })
        .and_then(|artifact| std::fs::read_to_string(artifact.path()).ok())
}

/// The directory the run started under `root` keeps its own state in,
/// or `None` before there is a run.
fn run_dir(root: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(runs_root(&home(root)))
        .ok()?
        .flatten()
        .next()
        .map(|run| run.path())
}

/// A run of [`ASKING`] on a terminal, its node having produced
/// `questions`.
fn asking(root: &Path, questions: &str) -> Terminal {
    let repo = root.join("repo");
    let home = home(root);
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    write(&repo.join(".yunta/config.yaml"), CONFIG);
    write(&repo.join("wf.yaml"), ASKING);
    write(
        &repo.join("fixture.yaml"),
        &WROTE_THEM.replace("QUESTIONS", &indent(questions, &" ".repeat(10))),
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "asking"]);
    // `--quiet` leaves the run's progress out: what a person is asked is
    // not progress, and the round is what these tests read.
    yunta_on_terminal!(
        &repo,
        &home,
        &[
            "run",
            "wf.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml",
            "--quiet",
        ]
    )
}

/// A run that reaches a gate with its re-routes already spent — an
/// escalation put to a person with no agent anywhere in it.
fn gated(root: &Path) -> Terminal {
    let repo = root.join("repo");
    let home = home(root);
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: gated\nnodes:\n  - id: lint\n    kind: bash\n    run: \"false\"\n    \
         on_failure: { goto: fix, max_reroutes: 0 }\n  - id: fix\n    kind: bash\n    \
         run: \"true\"\n",
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "gated"]);
    yunta_on_terminal!(&repo, &home, &["run", "wf.yaml"])
}

#[test]
fn a_console_prompt_does_not_stall_the_run_tools_listener() {
    let root = tempfile::tempdir().unwrap();
    let terminal = gated(root.path());
    terminal.wait_for("> 1  retry", "the gate never put its menu on the console");

    // While the prompt waits for an answer, a SIGINT must still be
    // handled: the Ctrl-C task shares the run's runtime with the console
    // read and the run-tools listener. A read that blocked the runtime
    // would starve every one of them, and this note would never print.
    terminal.interrupt();
    terminal.wait_for(
        "interrupt received",
        "SIGINT went unhandled while the console prompt waited — the read stalled the runtime",
    );
}

#[test]
fn the_live_region_comes_off_the_terminal_before_a_prompt_draws_on_it() {
    let root = tempfile::tempdir().unwrap();
    let terminal = gated(root.path());
    terminal.wait_for("nodes ", "the run never pinned its region to the terminal");
    terminal.wait_for(
        "a decision is needed",
        "the gate never put its decision on the console",
    );

    // The region and the prompt write to the same stream, and the one
    // that draws second lands on what the other put there. So the
    // region comes down first, and stays down until the prompt ends.
    assert!(
        terminal.cleared_before("nodes ", "a decision is needed"),
        "the region was still pinned to the terminal the prompt drew on:\n{}",
        terminal.drawn()
    );
}

#[test]
fn an_interrupt_at_an_open_prompt_stops_the_run_and_the_process_with_it() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = gated(root.path());
    terminal.wait_for("> 1  retry", "the gate never put its menu on the console");
    terminal.interrupt();

    // A person at the menu is no longer being asked anything. The read
    // stops waiting on them, the engine unwinds the run, and the
    // process leaves — rather than living on around a thread parked on
    // a key nobody is going to press.
    let drawn = terminal.ended();
    assert!(
        !terminal.ran_to_the_end(),
        "a run stopped by a person is not a success:\n{drawn}"
    );
    assert!(
        drawn.contains("interrupt received"),
        "the interrupt never reached the run's one cancellation bridge:\n{drawn}"
    );
    // The engine's own teardown ran to completion before the process
    // left: the run's close is on the log and in the block that reports
    // it, the checkout it held is unlocked, and its process tree is no
    // longer registered as running.
    assert!(
        drawn.contains("paused on a decision") && drawn.contains("waiting on node `lint`"),
        "the run's own close never reached the person who stopped it:\n{drawn}"
    );
    assert!(
        !root.path().join("repo/.git/yunta-none.lock").exists(),
        "the checkout this run held is still locked after it stopped"
    );
    let run_dir = run_dir(root.path()).expect("the run wrote its own directory");
    assert!(
        !run_dir.join("scratch/engine.json").exists(),
        "the run's process tree is still registered as running: {}",
        run_dir.display()
    );
    // The menu was left mid-read: the thread that would have put the
    // cursor back is still waiting on a key, so somebody else has to.
    assert!(
        terminal.cursor_is_back(),
        "the abandoned prompt left the terminal without its cursor:\n{drawn}"
    );
}

#[test]
fn a_keystroke_in_the_list_goes_back_over_the_rows_the_list_drew_and_no_further() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = gated(root.path());
    terminal.wait_for("2  abort", "the gate never put its options on the console");
    // Any key redraws the list, and the redraw begins by clearing the
    // rows it last wrote. Clearing more than it wrote takes the rows
    // above it — the evidence the decision is being made on.
    terminal.keys("\x1b[B");
    terminal.wait_for("> 2  abort", "the list never moved");

    let (drew, cleared) = terminal.drew_and_cleared("> 1  retry");
    assert_eq!(
        cleared,
        drew,
        "the list drew {drew} rows and went back over {cleared}:\n{}",
        terminal.drawn()
    );
}

#[test]
fn an_escalation_shows_its_evidence_above_the_options_it_offers() {
    let root = tempfile::tempdir().unwrap();
    let terminal = gated(root.path());
    terminal.wait_for(
        "tradeoff:",
        "the gate never put its options, or what they cost, on the console",
    );
    let drawn = terminal.drawn();
    let evidence = drawn
        .find("evidence, attached by the engine")
        .expect("the engine's own evidence is what the summary is audited against");
    let options = drawn.find("tradeoff:").unwrap_or_default();
    assert!(
        evidence < options,
        "the evidence is read before the options, not after them:\n{drawn}"
    );
}

#[test]
fn a_pasted_pair_of_lines_answers_one_question() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(
        root.path(),
        "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: text\n    \
         required: true\n  - id: risk\n    text: \"Any risk?\"\n    answer_type: text\n    \
         required: true\n",
    );
    terminal.wait_for("1/2  What changed?", "the first question was never asked");
    // What a terminal sends for a paste of two lines once it has been
    // asked to mark one: the break between them is inside the markers.
    terminal.keys("\x1b[200~first line\rsecond line\x1b[201~");
    terminal.wait_for(
        "first line second line",
        "the paste never reached the answer",
    );
    terminal.keys("\r");
    terminal.wait_for(
        "2/2  Any risk?",
        "the paste answered the second question as well as the first",
    );
    terminal.keys("none\r");

    let drawn = terminal.ended();
    assert!(
        drawn.contains("spanned more than one line"),
        "folding what was pasted into one line is said, never silent:\n{drawn}"
    );
    let answers = answers(root.path()).unwrap_or_default();
    assert!(
        answers.contains("value: first line second line") && answers.contains("value: none"),
        "one paste answered one question, and the next was asked on its own:\n{answers}"
    );
}

#[test]
fn an_answer_wider_than_the_terminal_is_typed_on_one_row() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(
        root.path(),
        "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: text\n    \
         required: true\n",
    );
    terminal.raw();
    terminal.wait_for("1/1  What changed?", "the question was never asked");
    // Half again as wide as the screen. A row redrawn wider than the
    // terminal wraps onto the row below, which the next redraw does not
    // clear, and every keystroke past the edge leaves another one
    // standing.
    let long: String = (0..12).map(|at| format!("{at:02}abcdefgh")).collect();
    terminal.keys(&long);
    terminal.wait_for("11abcdefgh", "the answer was never typed to its end");

    // Read while the line is still being edited: what closes a finished
    // answer is a line the terminal lays out over as many rows as it
    // takes, and it is drawn once, with nothing after it to redraw.
    for row in terminal.rows_redrawn() {
        assert!(
            row.chars().count() <= usize::from(Terminal::COLUMNS),
            "a row {} cells wide on a terminal {} wide: {row:?}",
            row.chars().count(),
            Terminal::COLUMNS
        );
    }

    terminal.keys("\r");
    terminal.ended();
    assert!(
        answers(root.path())
            .unwrap_or_default()
            .contains(&format!("value: {long}")),
        "every character typed is in the answer: {:?}",
        answers(root.path())
    );
}

#[test]
fn an_arrow_key_leaves_nothing_of_itself_in_the_answer() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(
        root.path(),
        "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: text\n    \
         required: true\n",
    );
    terminal.wait_for("1/1  What changed?", "the question was never asked");
    // The cursor goes back one character and types into the gap.
    terminal.keys("ac\x1b[Db\r");
    terminal.ended();
    assert!(
        answers(root.path())
            .unwrap_or_default()
            .contains("value: abc"),
        "the arrow moved the cursor and left no escape sequence behind: {:?}",
        answers(root.path())
    );
}

#[test]
fn escape_parks_the_run_with_nothing_recorded() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(
        root.path(),
        "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: text\n    \
         required: true\n",
    );
    terminal.wait_for("1/1  What changed?", "the question was never asked");
    terminal.keys("\x1b");

    let drawn = terminal.ended();
    assert!(
        drawn.contains("the run parks here"),
        "a declined prompt says what became of the run:\n{drawn}"
    );
    assert!(
        !terminal.ran_to_the_end(),
        "the run parks rather than deciding for the person:\n{drawn}"
    );
    assert_eq!(
        answers(root.path()),
        None,
        "a declined round records nothing at all"
    );
}

#[test]
fn a_choice_is_answered_by_arrow_or_by_typing_its_value() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(
        root.path(),
        "questions:\n  - id: env\n    text: \"Which environment?\"\n    answer_type: choice\n    \
         values: [staging, production]\n    required: true\n  - id: tier\n    text: \"Which \
         tier?\"\n    answer_type: choice\n    values: [basic, pro]\n    required: true\n",
    );
    terminal.wait_for(
        "1/2  Which environment?",
        "the first question was never asked",
    );
    terminal.wait_for("2  production", "the declared values were never offered");
    // Down one, then Enter.
    terminal.keys("\x1b[B\r");
    terminal.wait_for("2/2  Which tier?", "the first answer never landed");
    // The declared value, typed.
    terminal.keys("pro\r");

    terminal.ended();
    let answers = answers(root.path()).unwrap_or_default();
    assert!(
        answers.contains("value: production") && answers.contains("value: pro"),
        "an arrow and a typed value each answer a choice:\n{answers}"
    );
}

#[test]
fn an_answer_its_own_rules_refuse_is_asked_again() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(
        root.path(),
        "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: text\n    \
         required: true\n",
    );
    terminal.wait_for("1/1  What changed?", "the question was never asked");
    // An empty line answers nothing, which this question does not allow.
    terminal.keys("\r");
    terminal.wait_for(
        "required question `summary` has no answer",
        "the refusal a person reads is the engine's own",
    );
    terminal.keys("the auth middleware\r");

    terminal.ended();
    assert!(
        answers(root.path())
            .unwrap_or_default()
            .contains("value: the auth middleware"),
        "the answer that was accepted is the one recorded: {:?}",
        answers(root.path())
    );
}

#[test]
fn a_prompt_that_ends_on_an_interrupt_leaves_the_terminal_its_cursor() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = gated(root.path());
    terminal.raw();
    terminal.wait_for("> 1  retry", "the gate never put its menu on the console");
    // The list hides the cursor while it draws. Ending any way but
    // answered or declined and leaving it hidden hands the shell the run
    // returns to a terminal with no cursor, which nothing after it puts
    // back.
    terminal.keys("\x03");
    wait_until(
        || terminal.cursor_is_back(),
        || {
            format!(
                "the prompt ended on an interrupt without putting the cursor back\ndrawn so \
                 far:\n{}",
                terminal.drawn()
            )
        },
    );

    let drawn = terminal.ended();
    assert!(
        terminal.cursor_is_back(),
        "the cursor is put back as often as it is taken away:\n{drawn}"
    );
}

#[test]
fn a_typed_ctrl_c_stops_the_run_the_prompts_raw_mode_hid_it_from() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(
        root.path(),
        "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: text\n    \
         required: true\n",
    );
    terminal.raw();
    terminal.wait_for("1/1  What changed?", "the question was never asked");
    // Nothing but the prompt itself can put this byte back on the run's
    // one cancellation path now.
    terminal.keys("\x03");
    terminal.wait_for(
        "interrupt received",
        "a typed Ctrl-C was swallowed by the prompt instead of stopping the run",
    );

    let drawn = terminal.ended();
    assert!(
        !terminal.ran_to_the_end(),
        "a run stopped by a person is not a success:\n{drawn}"
    );
    assert_eq!(
        answers(root.path()),
        None,
        "a round a person stopped records nothing at all"
    );
}
