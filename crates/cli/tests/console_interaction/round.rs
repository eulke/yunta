//! A round of questions: each answered in its turn, and left with its answer.

use super::*;

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
