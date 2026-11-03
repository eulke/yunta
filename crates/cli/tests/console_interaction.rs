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

use yunta_core::events::{EventPayload, GateEvent, GateResolvedPayload, HumanChoice};
use yunta_testkit::{runs_root, wait_until, yunta_on_terminal, Checkout, Terminal};

/// What a decision put to a person points at first, by the option it offers first.
const POINTED: &str = "▸ ";

/// A node that asks: the session hands the questions over and ends, and
/// the round with the person happens after it closes.
const ASKING: &str = "\
name: asking
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: \"Ask what has to be known before going on.\"
    artifacts:
      produces: [questions]
";

/// The session that hands the questions over, scripted — the round a
/// person answers is reached with no agent installed.
const HANDED_THEM_OVER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
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

/// The answers the run under `root` recorded for its `ask` node, or
/// `None` while it has recorded none. The run's artifact view keeps a
/// producer's documents under its node, and the answers to a node's
/// questions are a document of that same node.
fn answers(root: &Path) -> Option<String> {
    let run = run_dir(root)?;
    std::fs::read_to_string(run.join("artifacts/ask/answers.yaml")).ok()
}

/// The choice the run started under `root` recorded at its gate, or
/// `None` while it has recorded none.
fn recorded(root: &Path) -> Option<HumanChoice> {
    let run_id = run_dir(root)?.file_name()?.to_str()?.parse().ok()?;
    let storage = yunta_storage::Storage::open(&home(root).join("yunta.db")).ok()?;
    let events = storage.events_for_run(&run_id).ok()?;
    events.iter().find_map(|event| match event.payload() {
        Some(EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(choice)))) => {
            Some(choice.clone())
        }
        _ => None,
    })
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

/// `questions` nested under the fixture's key, every line twelve spaces in.
fn nested(questions: &str) -> String {
    questions
        .lines()
        .map(|line| format!("{}{line}", " ".repeat(12)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A run of [`ASKING`] on a terminal, its node having produced
/// `questions`.
fn asking(root: &Path, questions: &str) -> Terminal {
    let checkout = Checkout::under(root)
        .config(CONFIG)
        .workflow("wf", ASKING)
        .file(
            "fixture.yaml",
            &HANDED_THEM_OVER.replace("QUESTIONS", &nested(questions)),
        )
        .committed();
    let (repo, home) = (checkout.repo, checkout.home);
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
    let checkout = Checkout::under(root)
        .working_in_place()
        .workflow(
            "wf",
            "name: gated\nnodes:\n  - id: lint\n    kind: bash\n    run: \"false\"\n    \
             on_failure: { goto: fix, max_reroutes: 0 }\n  - id: fix\n    kind: bash\n    \
             run: \"true\"\n",
        )
        .committed();
    let (repo, home) = (checkout.repo, checkout.home);
    yunta_on_terminal!(&repo, &home, &["run", "wf.yaml"])
}

/// A run that stops on an unresolved internal gate: the question its
/// author wrote is the escalation's summary, and who the gate is
/// assigned to — which that question never says — is the evidence the
/// engine attaches under it.
fn approving(root: &Path) -> Terminal {
    let checkout = Checkout::under(root)
        .working_in_place()
        .workflow(
            "wf",
            "name: approving\nnodes:\n  - id: ship\n    kind: gate\n    assignee: the release \
             lead\n    message: \"Ship what is on the branch?\"\n",
        )
        .committed();
    let (repo, home) = (checkout.repo, checkout.home);
    yunta_on_terminal!(&repo, &home, &["run", "wf.yaml"])
}

#[test]
fn a_console_prompt_does_not_stall_the_run_tools_listener() {
    let root = tempfile::tempdir().unwrap();
    let terminal = gated(root.path());
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");

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
fn the_prompt_names_no_worktree() {
    let root = tempfile::tempdir().unwrap();
    let terminal = gated(root.path());
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");
    // Where the run works is a fact about the run, which `yunta status`
    // says; the prompt says what the decision is about and what each
    // option costs.
    let screen = terminal.screen();
    assert!(!screen.holds("the run works in"), "{screen:?}");
}

#[test]
fn choosing_an_option_that_asks_nothing_records_it_with_one_key() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = approving(root.path());
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    terminal.keys("\r");

    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    let choice = recorded(root.path()).expect("the run recorded no choice");
    assert_eq!(choice.option.as_str(), "approve");
    assert_eq!(choice.free_text, None, "nothing was added to the choice");
}

#[test]
fn pressing_n_adds_a_note() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = approving(root.path());
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    terminal.keys("n");
    terminal.wait_on_screen("enter records it", "`n` never opened a line for the note");
    terminal.keys("ship it after lunch\r");

    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    let choice = recorded(root.path()).expect("the run recorded no choice");
    assert_eq!(choice.free_text.as_deref(), Some("ship it after lunch"));
}

#[test]
fn the_live_region_comes_off_the_terminal_before_a_prompt_draws_on_it() {
    let root = tempfile::tempdir().unwrap();
    let terminal = gated(root.path());
    terminal.wait_for("nodes ", "the run never pinned its region to the terminal");
    terminal.wait_on_screen(POINTED, "the gate never put its decision on the console");

    // The region and the prompt write to the same stream, and the one
    // that draws second lands on what the other put there. So the
    // region comes down first, and stays down until the prompt ends.
    assert!(
        terminal.cleared_before("nodes ", POINTED),
        "the region was still pinned to the terminal the prompt drew on:\n{}",
        terminal.drawn()
    );
}

#[test]
fn an_interrupt_at_an_open_prompt_stops_the_run_and_the_process_with_it() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = gated(root.path());
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");
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
        drawn.contains("decision on node `lint`") && drawn.contains("holds its own state on disk"),
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

/// A key redraws the card over itself: what stands above it stays, and the card
/// is there once.
#[test]
fn a_keystroke_in_the_card_redraws_it_in_place() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = gated(root.path());
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");
    wait_until(
        || !terminal.line_discipline_is_back(),
        || "the gate did not take the terminal before reading keys".into(),
    );
    let before = terminal.screen();
    let top = before
        .rows
        .iter()
        .position(|row| row.contains("▸ retry"))
        .unwrap_or(0);
    terminal.keys("\x1b[B");
    terminal.wait_on_screen("▸ abort", "the pointer never moved");

    let after = terminal.screen();
    assert_eq!(after.count(POINTED), 1, "{after:#?}");
    assert_eq!(after.count("retry"), before.count("retry"), "{after:#?}");
    assert_eq!(after.rows[..top], before.rows[..top], "{after:#?}");
    assert!(after.cursor_hidden, "no field is open: {after:#?}");
}

/// A run whose gate opens after a moment, for what is typed before it.
fn gated_after_a_moment(root: &Path) -> Terminal {
    let checkout = Checkout::under(root)
        .working_in_place()
        .workflow(
            "wf",
            "name: late\nnodes:\n  - id: wait\n    kind: bash\n    run: \"sleep 2\"\n  - id: ship\n    \
             kind: gate\n    depends_on: [wait]\n    assignee: lead\n    message: \"Ship it?\"\n",
        )
        .committed();
    let (repo, home) = (checkout.repo, checkout.home);
    yunta_on_terminal!(&repo, &home, &["run", "wf.yaml"])
}

/// With one key answering, an Enter typed while the run worked would answer a
/// question nobody had read.
#[test]
fn a_key_typed_before_the_question_answers_nothing() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = gated_after_a_moment(root.path());
    terminal.keys("\r\r");
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    wait_until(
        || !terminal.line_discipline_is_back(),
        || "the gate did not take the terminal before reading keys".into(),
    );
    assert!(
        terminal.screen().holds("▸ approve"),
        "{:#?}",
        terminal.screen()
    );
    assert!(recorded(root.path()).is_none(), "nobody answered it");
    terminal.keys("\r");

    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
}

/// A line break in pasted text is the byte an Enter is, and a card takes neither
/// a paste nor anything in it for an answer.
#[test]
fn a_paste_answers_no_decision() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = approving(root.path());
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    terminal.keys("\x1b[200~n\rmore\r\x1b[201~");
    terminal.keys("\x1b[B");
    terminal.wait_on_screen("▸ abort", "the card stopped reading keys after the paste");
    assert!(
        !terminal.screen().holds("enter records it"),
        "{:#?}",
        terminal.screen()
    );
    assert!(
        recorded(root.path()).is_none(),
        "a paste answered the decision"
    );
}

/// A run stopped on a gate that offers to go back with words: only a session picks
/// its work back up with what it is told, so the node it goes back to is an agent's.
fn adjusting(root: &Path) -> Terminal {
    let checkout = Checkout::under(root)
        .config(CONFIG)
        .workflow(
            "wf",
            "name: adjusting\nnodes:\n  - id: plan\n    kind: prompt\n    runner: executor\n    \
             prompt: \"Plan it.\"\n  - id: ok\n    kind: gate\n    depends_on: [plan]\n    \
             assignee: lead\n    message: \"Good?\"\n    options: [approve, adjust]\n    on: { \
             adjust: plan }\n",
        )
        .file(
            "fixture.yaml",
            "dialect: claude-code\nsessions:\n  - steps:\n      - { type: note, text: \"one\" }\n    \
             outcome: { type: completed, summary: planned }\n  - steps:\n      - { type: note, \
             text: \"two\" }\n    outcome: { type: completed, summary: planned again }\n",
        )
        .committed();
    let (repo, home) = (checkout.repo, checkout.home);
    yunta_on_terminal!(
        &repo,
        &home,
        &[
            "run",
            "wf.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml"
        ]
    )
}

/// An option that asks opens its field where the card stands, with the cursor on
/// it; esc goes back to the list, and an empty answer is said back, not sent.
#[test]
fn an_option_that_asks_opens_its_field_in_place() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = adjusting(root.path());
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    terminal.keys("\x1b[B");
    terminal.wait_on_screen("▸ adjust", "the pointer never moved");
    terminal.keys("\r");
    terminal.wait_on_screen("what should change?", "the option never asked for words");
    let screen = terminal.screen();
    assert!(
        !screen.cursor_hidden,
        "a field shows its cursor: {screen:#?}"
    );
    let (row, column) = screen.cursor;
    assert!(
        screen.rows[usize::from(row)].starts_with("▎ >") && column == 4,
        "the cursor stands on the field, past its prompt: {screen:#?}"
    );
    assert!(
        !screen.holds("approve"),
        "the field stands in for the list: {screen:#?}"
    );

    terminal.keys("\r");
    terminal.wait_on_screen("this option needs an answer", "an empty answer was sent");
    terminal.keys("half");
    terminal.keys("\x1b");
    terminal.wait_on_screen("enter chooses", "esc never went back to the list");
    let screen = terminal.screen();
    assert!(
        screen.holds("▸ adjust"),
        "the pointer stays where it was: {screen:#?}"
    );
    assert!(screen.cursor_hidden, "{screen:#?}");
    assert!(recorded(root.path()).is_none(), "nothing was answered");
}

/// Answered, the card is gone from the screen and what was chosen is left in the
/// history, with the words it was said with.
#[test]
fn an_answered_card_leaves_what_was_chosen() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = adjusting(root.path());
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    terminal.keys("\x1b[B");
    terminal.wait_on_screen("▸ adjust", "the pointer never moved");
    terminal.keys("\r");
    terminal.wait_on_screen("what should change?", "the option never asked for words");
    terminal.keys("keep the lock\r");
    terminal.wait_for(
        "chose `adjust`: keep the lock",
        "what was chosen was not left in the history",
    );
    // The gate asks again once `plan` has run again.
    terminal.wait_on_screen("▸ approve", "the gate never asked again");
    let screen = terminal.screen();
    assert_eq!(screen.count(POINTED), 1, "one card at a time: {screen:#?}");
    assert!(!screen.holds("what should change?"), "{screen:#?}");
}

/// A terminal that declares itself dumb has said it draws nothing beyond text, so
/// nobody is asked there: the run parks and says the command for each option.
#[test]
fn a_dumb_terminal_is_asked_nothing_and_the_run_parks_with_its_commands() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .working_in_place()
        .workflow(
            "wf",
            "name: approving\nnodes:\n  - id: ship\n    kind: gate\n    assignee: lead\n    \
             message: \"Ship it?\"\n",
        )
        .committed();
    let mut terminal = yunta_on_terminal!(
        &checkout.repo,
        &checkout.home,
        &["run", "wf.yaml"],
        &[("TERM", "dumb")]
    );
    let drawn = terminal.ended();
    assert_eq!(terminal.exit_code(), Some(3), "{drawn}");
    assert!(!drawn.contains(POINTED), "{drawn}");
    assert!(drawn.contains("resolve-gate"), "{drawn}");
}

/// Stopping the run takes a round off the screen, what was typed with it: nothing of
/// it was recorded, and what the run says next is not read under a question.
#[test]
fn an_interrupt_takes_a_round_off_the_screen() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(
        root.path(),
        "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: text\n    \
         required: true\n",
    );
    terminal.wait_on_screen("◆ What changed?", "the question was never asked");
    terminal.keys("half an ans");
    terminal.wait_on_screen("> half an ans", "the words never reached the line");
    terminal.keys("\x03");
    let drawn = terminal.ended();
    let screen = terminal.screen();
    assert!(!screen.holds("half an ans"), "{screen:#?}\n{drawn}");
    assert!(!screen.holds("What changed?"), "{screen:#?}\n{drawn}");
    assert!(terminal.line_discipline_is_back(), "{drawn}");
}

/// Two typed questions, for what a round does between them.
const TWO_TYPED: &str =
    "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: \
                         text\n    required: true\n  - id: risk\n    text: \"Any risk?\"\n    \
                         answer_type: text\n    required: true\n";

/// Going back finds what was said, and what is said then is what is recorded.
#[test]
fn shift_tab_goes_back_and_the_answer_can_change() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(root.path(), TWO_TYPED);
    terminal.wait_on_screen("◆ What changed?", "the first question was never asked");
    terminal.keys("one\r");
    terminal.wait_on_screen("◆ Any risk?", "the first answer never landed");
    assert!(
        terminal.screen().holds("✓ What changed?  one"),
        "{:#?}",
        terminal.screen()
    );
    terminal.keys("\x1b[Z");
    terminal.wait_on_screen("◆ What changed?", "shift-tab never went back");
    assert!(terminal.screen().holds("> one"), "{:#?}", terminal.screen());
    terminal.keys("\x7f\x7f\x7ftwo\r");
    terminal.wait_on_screen("◆ Any risk?", "the changed answer never landed");
    terminal.keys("none\r");

    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    let recorded = answers(root.path()).unwrap_or_default();
    assert!(
        recorded.contains("value: two") && !recorded.contains("value: one"),
        "what was said last is what is recorded:\n{recorded}"
    );
}

/// A key pressed for one question is no answer to the next.
#[test]
fn a_key_typed_for_one_question_does_not_answer_the_next() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(
        root.path(),
        "questions:\n  - id: go\n    text: \"Go on?\"\n    answer_type: boolean\n    required: \
         true\n  - id: env\n    text: \"Which environment?\"\n    answer_type: choice\n    \
         values: [staging, production]\n    required: true\n",
    );
    terminal.wait_on_screen("◆ Go on?", "the first question was never asked");
    // A second Enter in the same breath as the first.
    terminal.keys("\r\r");
    terminal.wait_on_screen("◆ Which environment?", "the first answer never landed");
    terminal.keys("\x1b");
    let drawn = terminal.ended();
    assert!(
        drawn.contains("the run parks here"),
        "the second question was still being asked when esc left it:\n{drawn}"
    );
    assert_eq!(answers(root.path()), None, "the second Enter answered it");
}

/// When a round ends, each question stays in the history with what it was answered,
/// whole, and the drawing is gone.
#[test]
fn a_round_leaves_each_question_with_its_answer() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(root.path(), TWO_TYPED);
    terminal.wait_on_screen("◆ What changed?", "the first question was never asked");
    terminal.keys("one\r");
    terminal.wait_on_screen("◆ Any risk?", "the first answer never landed");
    terminal.keys("none\r");
    let drawn = terminal.ended();
    assert!(drawn.contains("✓ What changed?  one"), "{drawn}");
    assert!(drawn.contains("✓ Any risk?  none"), "{drawn}");
    let screen = terminal.screen();
    assert!(!screen.holds("◆ Any risk?"), "{screen:#?}");
}

/// Stopping the run takes the card off the screen before the closing draws, so
/// what the run leaves is not read under a question nobody can answer any more.
#[test]
fn an_interrupt_takes_the_card_off_the_screen() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = approving(root.path());
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    terminal.interrupt();
    let drawn = terminal.ended();
    let screen = terminal.screen();
    assert_eq!(screen.count(POINTED), 0, "{screen:#?}\n{drawn}");
    assert!(terminal.cursor_is_back(), "{drawn}");
    assert!(terminal.line_discipline_is_back(), "{drawn}");
}

#[test]
fn an_escalation_says_what_happened_above_the_options_it_offers() {
    let root = tempfile::tempdir().unwrap();
    let terminal = gated(root.path());
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");
    let drawn = terminal.drawn();
    let happened = drawn
        .find("re-routes to `fix` are exhausted")
        .expect("a decision opens with the account of what raised it");
    let options = drawn.find("▸ retry").unwrap_or_default();
    assert!(
        happened < options,
        "what happened is read before what to do about it, not after it:\n{drawn}"
    );
}

#[test]
fn an_escalation_says_the_record_behind_its_claim_once() {
    let root = tempfile::tempdir().unwrap();
    let terminal = gated(root.path());
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");
    let drawn = terminal.drawn();

    // The claim says what happened and the record says what the log
    // holds about it. A person at a prompt audits the first against the
    // second, which only works while the account they are reading is
    // not two copies of one sentence.
    assert!(
        drawn.contains("re-routes to `fix` are exhausted"),
        "the account of what raised the decision never reached the console:\n{drawn}"
    );
    // Scoped to the decision itself: the node's own failure line said
    // `exit 1` too, and that is a different surface reporting a
    // different thing, not this block saying one thing twice.
    let decision = &drawn[drawn
        .find("re-routes to `fix` are exhausted")
        .expect("the prompt never opened its card")..];
    assert_eq!(
        decision.matches("exit 1").count(),
        1,
        "the failure is the record behind the claim, not also the claim:\n{decision}"
    );
}

#[test]
fn an_escalation_draws_the_evidence_the_engine_attached_above_the_options() {
    let root = tempfile::tempdir().unwrap();
    let terminal = approving(root.path());
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    let drawn = terminal.drawn();

    // The summary is an account of what happened and the evidence is
    // the engine's own record of it. A menu offered without the record
    // asks for a decision on a claim nobody checked, so the record is
    // read first — and this gate's record says what its summary never
    // does, which is who is being asked.
    let claim = drawn
        .find("Ship what is on the branch?")
        .unwrap_or_else(|| panic!("the prompt never said what it asks:\n{drawn}"));
    let attached = drawn
        .find("assignee: the release lead")
        .unwrap_or_else(|| panic!("the prompt drew no evidence at all:\n{drawn}"));
    let options = drawn.find("▸ approve").expect("no option rows");
    assert!(
        claim < attached,
        "the record is read under the claim it audits:\n{drawn}"
    );
    assert!(
        attached < options,
        "the record a decision is audited against is read after the options it audits:\n{drawn}"
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
    terminal.wait_on_screen("◆ What changed?", "the first question was never asked");
    // What a terminal sends for a paste of two lines once it has been
    // asked to mark one: the break between them is inside the markers.
    terminal.keys("\x1b[200~first line\rsecond line\x1b[201~");
    terminal.wait_on_screen(
        "first line second line",
        "the paste never reached the answer",
    );
    assert!(
        !terminal.screen().holds("◆ Any risk?"),
        "the paste answered the first question by itself"
    );
    terminal.keys("\r");
    terminal.wait_on_screen("◆ Any risk?", "the first question was never answered");
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
    terminal.wait_on_screen("◆ What changed?", "the question was never asked");
    // Half again as wide as the screen. A row redrawn wider than the
    // terminal wraps onto the row below, which the next redraw does not
    // clear, and every keystroke past the edge leaves another one
    // standing.
    let long: String = (0..12).map(|at| format!("{at:02}abcdefgh")).collect();
    terminal.keys(&long);
    terminal.wait_for("11abcdefgh", "the answer was never typed to its end");

    // Read while the line is still being edited: the row holds the end of it, and what
    // stands under the row is what stood there before it grew.
    let screen = terminal.screen();
    let row = screen
        .rows
        .iter()
        .position(|row| row.contains("11abcdefgh"))
        .unwrap_or_else(|| panic!("the end of the answer is off the screen: {screen:#?}"));
    assert_eq!(screen.count("11abcdefgh"), 1, "{screen:#?}");
    assert_eq!(screen.rows[row + 1], "▎", "the row wrapped: {screen:#?}");

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
    terminal.wait_on_screen("◆ What changed?", "the question was never asked");
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
    terminal.wait_on_screen("◆ What changed?", "the question was never asked");
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
    terminal.wait_on_screen("◆ Which environment?", "the first question was never asked");
    terminal.wait_on_screen("▸ staging", "the declared values were never offered");
    // Down one, then Enter.
    terminal.keys("\x1b[B\r");
    terminal.wait_on_screen("◆ Which tier?", "the first answer never landed");
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
    terminal.wait_on_screen("◆ What changed?", "the question was never asked");
    // An empty line answers nothing, which this question does not allow.
    terminal.keys("\r");
    terminal.wait_for(
        "is `required` and nothing answers it",
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
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");
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
fn a_prompt_abandoned_mid_read_leaves_the_terminal_reading_and_echoing_again() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = gated(root.path());
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");
    // The read turns the line discipline off for as long as it reads,
    // and this is the moment it has: waiting on it is what makes the
    // interrupt below land on a prompt that holds the terminal.
    wait_until(
        || !terminal.line_discipline_is_back(),
        || {
            format!(
                "the prompt never took the terminal's line discipline\ndrawn so far:\n{}",
                terminal.drawn()
            )
        },
    );

    terminal.interrupt();

    // The read is abandoned where it stands: the thread that would put
    // the line discipline back is still waiting on a key nobody is
    // going to press, so the process does it on its way out. Left as
    // the read left it, the shell this run returns to echoes nothing a
    // person types into it.
    let drawn = terminal.ended();
    assert!(
        terminal.line_discipline_is_back(),
        "the abandoned prompt left the terminal with its echo and line editing \
         off:\n{drawn}"
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
    terminal.wait_on_screen("◆ What changed?", "the question was never asked");
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

#[test]
fn init_asks_through_the_console_and_honours_escape() {
    // `yunta init -i` asks on the one line every prompt in this binary
    // is answered on, so the gestures that answer a run answer this
    // too: Enter takes what was typed, and Escape — "not me, not now" —
    // takes the value `init` detected on its own.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let home = root.path().join("state");

    let mut terminal = yunta_on_terminal!(&repo, &home, &["init", "-i"]);
    terminal.wait_for("project name", "init never asked for a project name");
    terminal.keys("orchard\r");
    terminal.wait_for("base branch", "init never asked for a base branch");
    terminal.keys("\x1b");

    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    assert!(
        drawn.contains("esc keeps the detected default"),
        "every prompt says what Escape does — and this one has no run to \
         park, so it says the other thing:\n{drawn}"
    );

    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap();
    assert!(
        config.contains("name: orchard"),
        "the answer that was typed is what was written:\n{config}"
    );
    let kept = base_branch_in(&config);
    assert!(
        drawn.contains(&format!("> {kept}")),
        "the field never held it, so this is what was left being said with the value \
         it kept:\n{drawn}"
    );
}

/// The base branch a `.yunta/config.yaml` records.
fn base_branch_in(config: &str) -> &str {
    config
        .lines()
        .find_map(|line| line.trim().strip_prefix("base_branch: "))
        .expect("the configuration records a base branch")
}

#[test]
fn init_says_the_value_a_line_left_empty_keeps() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let home = root.path().join("state");

    let mut terminal = yunta_on_terminal!(&repo, &home, &["init", "-i"]);
    terminal.wait_for("project name", "init never asked for a project name");
    terminal.keys("\r");
    terminal.wait_for("base branch", "init never asked for a base branch");
    terminal.keys("\r");

    let drawn = terminal.ended();
    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap();
    assert!(config.contains("name: repo"), "{config}");
    assert!(drawn.contains("> repo"), "the name it kept:\n{drawn}");
    let kept = base_branch_in(&config);
    assert!(
        drawn.contains(&format!("> {kept}")),
        "the branch it kept:\n{drawn}"
    );
}

#[test]
fn ctrl_c_at_init_stops_it_with_nothing_written_or_said() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let home = root.path().join("state");

    let mut terminal = yunta_on_terminal!(&repo, &home, &["init", "-i"]);
    terminal.wait_for("project name", "init never asked for a project name");
    terminal.keys("\x03");

    let drawn = terminal.ended();
    assert_eq!(terminal.exit_code(), Some(130), "{drawn}");
    assert!(
        !drawn.contains("base branch"),
        "nothing more was asked:\n{drawn}"
    );
    assert!(
        !drawn.contains("> repo"),
        "a prompt that was stopped says no value:\n{drawn}"
    );
    assert!(
        !repo.join(".yunta/config.yaml").exists(),
        "and nothing was written:\n{drawn}"
    );
    assert!(terminal.line_discipline_is_back(), "{drawn}");
    assert!(terminal.cursor_is_back(), "{drawn}");
}

#[test]
fn the_questions_say_which_node_asks_and_how_many_answers_it_needs() {
    let root = tempfile::tempdir().unwrap();
    let terminal = asking(
        root.path(),
        "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: text\n    \
         required: true\n",
    );
    terminal.wait_on_screen(
        "1 answer before `ask` goes on",
        "the round never said who asks and how many answers it needs",
    );
}
