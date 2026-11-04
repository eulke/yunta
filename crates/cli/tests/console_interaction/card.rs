//! A decision's card: what it says, how it is answered and what it leaves behind.

use super::*;

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
    adjusting_with(root, &[])
}

/// [`adjusting`], with `flags` on its command line.
fn adjusting_with(root: &Path, flags: &[&str]) -> Terminal {
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
    let mut args = vec![
        "run",
        "wf.yaml",
        "--adapter",
        "mock",
        "--fixture",
        "fixture.yaml",
    ];
    args.extend_from_slice(flags);
    yunta_on_terminal!(&repo, &home, &args)
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
        "rerouted to `plan`: gate `ok` chose `adjust`: keep the lock",
        "what was chosen was not left in the history",
    );
    // The gate asks again once `plan` has run again.
    terminal.wait_on_screen("▸ approve", "the gate never asked again");
    let screen = terminal.screen();
    assert_eq!(screen.count(POINTED), 1, "one card at a time: {screen:#?}");
    assert!(!screen.holds("what should change?"), "{screen:#?}");
    // The chronicle says who chose and where it sends the run, once, and a choice is no
    // failure.
    let drawn = terminal.drawn();
    assert!(drawn.contains("`adjust` chosen by"), "{drawn}");
    assert!(!drawn.contains("ok failed"), "{drawn}");
    assert!(!drawn.contains("attributed to"), "{drawn}");
    assert!(!drawn.contains("\nchose `adjust`"), "{drawn}");
}

/// With `--quiet` nothing chronicles the run, so what was chosen and whom it is recorded
/// under are said where it was chosen.
#[test]
fn a_quiet_run_says_what_was_chosen_where_it_was_chosen() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = adjusting_with(root.path(), &["--quiet"]);
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
    terminal.wait_for("attributed to", "nobody was named for the choice");
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
        .find("reroutes to `fix` are exhausted")
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
        drawn.contains("reroutes to `fix` are exhausted"),
        "the account of what raised the decision never reached the console:\n{drawn}"
    );
    // Scoped to the decision itself: the node's own failure line said
    // `exit 1` too, and that is a different surface reporting a
    // different thing, not this block saying one thing twice.
    let decision = &drawn[drawn
        .find("reroutes to `fix` are exhausted")
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
