//! The plan's desk on a real terminal: what `r` opens from a decision's card, what
//! `yunta review` opens on a run that waits on one, and what each leaves behind.

use std::path::Path;

use yunta_testkit::{
    run_id_from, stderr, stdout, yunta_in, yunta_on_terminal, yunta_on_terminal_sized, Checkout,
    Terminal,
};

const REVIEWED: &str = r#"
name: reviewed
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    message: "Plan registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows: [{ node: plan, kind: tasks }]
"#;

const PLANNER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Make the two files"
            description: "Writes the files the run is about."
            decisions:
              - { id: names, question: "What are they called?", choice: "made and said", alternatives: [a and b], why: "They say what they are." }
            risks: ["Two files can drift apart."]
            tasks:
              - { id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], changes: [{ at: made.txt, what: "the file", code: "the file's first line" }], outcome: "made.txt exists", criteria: [{ cmd: "test -f made.txt", proves: "the file exists" }] }
              - { id: T002, title: "Say it", description: "Writes said.txt.", scope: [said.txt], depends_on: [T001], changes: [{ at: said.txt, what: "the words", code: "the words themselves" }], outcome: "said.txt exists", criteria: [{ cmd: "test -f said.txt", proves: "the words are there" }] }
    outcome: { type: completed, summary: "planned" }
  - steps:
      - { type: note, text: "planned again" }
    outcome: { type: completed, summary: "planned again" }
"#;

const RUN: [&str; 7] = [
    "run",
    "wf.yaml",
    "--adapter",
    "mock",
    "--fixture",
    "fixture.yaml",
    "--quiet",
];

/// A terminal with room for the rail.
const WIDE: (u16, u16) = (118, 36);

fn checkout(root: &Path, workflow: &str) -> Checkout {
    Checkout::under(root)
        .config(
            "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
             - { adapter: mock, model: mock-model }\n",
        )
        .workflow("wf", workflow)
        .file("fixture.yaml", PLANNER)
        .committed()
}

/// A run stopped on the plan's card, on a terminal with room for the rail.
fn at_the_card(checkout: &Checkout) -> Terminal {
    let terminal = yunta_on_terminal_sized!(&checkout.repo, &checkout.home, &RUN, WIDE);
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    terminal
}

/// A run parked on the plan's decision, off a terminal, and its id.
fn parked(checkout: &Checkout) -> String {
    let run = yunta_in!(&checkout.repo, &checkout.home, &RUN);
    assert_eq!(run.status.code(), Some(3), "{}", stderr(&run));
    run_id_from(&run)
}

/// What the card says of the plan is what it is about and how big it is; the plan itself
/// is a key away, and the page above the card does not carry it.
#[test]
fn the_card_says_what_the_plan_is_and_offers_it_with_a_key() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let terminal = at_the_card(&checkout);
    let screen = terminal.screen();
    assert!(screen.holds("Make the two files"), "{screen:#?}");
    assert!(screen.holds("2 tasks in 2 steps"), "{screen:#?}");
    assert!(screen.holds("r reads the plan"), "{screen:#?}");
    assert!(!screen.alternate, "it never opens by itself: {screen:#?}");
    let drawn = terminal.drawn();
    assert!(!drawn.contains("T001: Make it"), "{drawn}");
    assert!(!drawn.contains("decision · needs you"), "{drawn}");
}

/// `r` puts the plan on the other screen, and `esc` comes back to a card that is where it
/// was, and to what was on the screen before it.
#[test]
fn r_opens_the_desk_on_the_other_screen_and_esc_comes_back_to_the_card() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let mut terminal = at_the_card(&checkout);
    let before = terminal.screen();
    terminal.keys("r");
    terminal.wait_on_screen("▸ the change", "the desk never opened");
    let desk = terminal.screen();
    assert!(desk.alternate, "{desk:#?}");
    assert!(desk.cursor_hidden, "{desk:#?}");
    assert!(desk.holds("reviewed (default)"), "{desk:#?}");
    assert!(desk.holds("▸ the change"), "{desk:#?}");
    assert!(
        desk.holds("Writes the files the run is about."),
        "{desk:#?}"
    );
    assert!(desk.holds("decided for you"), "{desk:#?}");
    assert!(desk.holds("T001 → T002"), "the order: {desk:#?}");
    assert!(desk.holds("│"), "{desk:#?}");
    assert!(desk.holds("1 approve   2 adjust   3 abort"), "{desk:#?}");

    terminal.keys("\x1b");
    terminal.wait_on_screen("▸ approve", "the card never came back");
    let after = terminal.screen();
    assert!(!after.alternate, "{after:#?}");
    assert_eq!(after.rows, before.rows, "it is where it was");
    let drawn = terminal.drawn();
    assert!(
        drawn.contains("\u{1b}[?1049h") && drawn.contains("\u{1b}[?1049l"),
        "{drawn}"
    );
    assert!(
        drawn.contains("\u{1b}[?1007h") && drawn.contains("\u{1b}[?1007r"),
        "the wheel scrolls it, and what the terminal had is put back: {drawn}"
    );
    assert!(!drawn.contains("\u{1b}[?1000"), "the mouse is not taken");
}

#[test]
fn j_walks_the_tasks_p_turns_a_card_over_and_d_shows_what_was_decided() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let mut terminal = at_the_card(&checkout);
    terminal.keys("r");
    terminal.wait_on_screen("▸ the change", "the desk never opened");
    terminal.keys("j");
    terminal.wait_on_screen("▸ T001", "j never moved the pointer");
    assert!(terminal.screen().holds("Make it"));
    assert!(terminal.screen().holds("step 1 of 2"));
    assert!(terminal.screen().holds("the file's first line"));
    terminal.keys("j");
    terminal.wait_on_screen("▸ T002", "j never moved the pointer again");
    assert!(terminal.screen().holds("the words themselves"));
    terminal.keys("p");
    terminal.wait_on_screen("proof · T002", "p never turned the card over");
    assert!(terminal.screen().holds("test -f said.txt"));
    terminal.keys("d");
    // The rail says `decided for you` already: what only the view holds is what to wait on.
    terminal.wait_on_screen(
        "▸ 1 What are they called?",
        "d never showed what was decided",
    );
    let screen = terminal.screen();
    assert!(screen.holds("made and said"), "{screen:#?}");
    assert!(screen.holds("Two files can drift apart."), "{screen:#?}");
    terminal.keys("d");
    terminal.wait_on_screen("Say it", "d never came back to the task");
}

/// Reading a plan is no time for one key to approve it: a number says what it would do,
/// and `enter` does it.
#[test]
fn a_number_confirmed_answers_the_decision_from_the_desk() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let mut terminal = at_the_card(&checkout);
    terminal.keys("r");
    terminal.wait_on_screen("▸ the change", "the desk never opened");
    terminal.keys("1");
    terminal.wait_on_screen("enter confirms", "a number never asked to be confirmed");
    assert!(terminal.screen().alternate, "nothing is answered yet");
    terminal.keys("\r");
    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    assert!(drawn.contains("chose `approve`"), "{drawn}");
    assert!(!terminal.screen().alternate, "{drawn}");
}

/// An option that asks for words asks where words are asked for.
#[test]
fn an_option_that_asks_comes_back_to_the_card_with_its_field_open() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let mut terminal = at_the_card(&checkout);
    terminal.keys("r");
    terminal.wait_on_screen("▸ the change", "the desk never opened");
    terminal.keys("2");
    terminal.wait_on_screen("enter confirms", "a number never asked to be confirmed");
    terminal.keys("\r");
    terminal.wait_on_screen("what should change?", "the field never opened");
    let screen = terminal.screen();
    assert!(!screen.alternate, "{screen:#?}");
    assert!(!screen.cursor_hidden, "{screen:#?}");
    assert!(screen.holds("▸ adjust"), "{screen:#?}");
}

/// What the run says of being stopped lands where the card stood, on the screen that
/// stays.
#[test]
fn ctrl_c_leaves_the_other_screen_before_the_run_stops() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let mut terminal = at_the_card(&checkout);
    terminal.keys("r");
    terminal.wait_on_screen("▸ the change", "the desk never opened");
    terminal.keys("\x03");
    let drawn = terminal.ended();
    assert_eq!(terminal.exit_code(), Some(130), "{drawn}");
    assert!(!terminal.screen().alternate, "{drawn}");
    assert!(terminal.line_discipline_is_back(), "{drawn}");
    assert!(terminal.cursor_is_back(), "{drawn}");
    let left = drawn
        .rfind("\u{1b}[?1049l")
        .expect("it left the other screen");
    assert!(
        !drawn[..left]
            .rsplit("\u{1b}[?1049h")
            .next()
            .unwrap_or_default()
            .contains("stopped"),
        "nothing is said on the screen that goes away: {drawn}"
    );
}

/// Under ninety columns the pane has the screen, and says which task it is.
#[test]
fn a_narrow_terminal_has_no_rail() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN);
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    terminal.keys("r");
    terminal.wait_on_screen("the change", "the desk never opened");
    terminal.keys("j");
    terminal.wait_on_screen("task 1 of 2", "the pane never said where it is");
    let screen = terminal.screen();
    assert!(screen.alternate, "{screen:#?}");
    assert!(!screen.holds("▸ T001"), "{screen:#?}");
    assert!(!screen.holds("impact"), "{screen:#?}");
}

/// A terminal with no room for a desk prints the plan in the history instead, and the card
/// stands under it.
#[test]
fn a_terminal_too_short_prints_the_plan_above_the_card() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let mut terminal = yunta_on_terminal_sized!(&checkout.repo, &checkout.home, &RUN, (90, 11));
    terminal.wait_on_screen("▸ approve", "the gate never put its card on the console");
    terminal.keys("r");
    terminal.wait_for("T002: Say it", "the plan was never printed");
    terminal.wait_on_screen("▸ approve", "the card never came back");
    assert!(!terminal.screen().alternate);
    let drawn = terminal.drawn();
    let last = drawn.rfind("T002: Say it").expect("the plan");
    assert!(
        drawn[last..].contains("Two files can drift apart."),
        "what weighs on the decision is said last, over the card: {drawn}"
    );
}

/// A gate with no plan has nothing for `r` to open, and draws what it shows above it.
#[test]
fn a_decision_with_no_plan_offers_no_key_for_one() {
    let root = tempfile::tempdir().unwrap();
    let workflow = REVIEWED.replace("    shows: [{ node: plan, kind: tasks }]\n", "");
    let checkout = checkout(root.path(), &workflow);
    let mut terminal = at_the_card(&checkout);
    assert!(!terminal.screen().holds("r reads the plan"));
    terminal.keys("r");
    terminal.keys("\x1b[B");
    terminal.wait_on_screen("▸ adjust", "the card stopped answering");
    assert!(!terminal.screen().alternate);
}

/// `yunta review` opens the desk at once, and leaving it leaves the command, with the run
/// as it was.
#[test]
fn review_opens_the_desk_and_esc_leaves_the_run_waiting() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let run = parked(&checkout);
    let mut terminal = yunta_on_terminal_sized!(
        &checkout.repo,
        &checkout.home,
        &["review", run.as_str()],
        WIDE
    );
    terminal.wait_on_screen("▸ the change", "review never opened the desk");
    assert!(terminal.screen().alternate);
    terminal.keys("\x1b");
    let drawn = terminal.ended();
    assert_eq!(terminal.exit_code(), Some(0), "{drawn}");
    assert!(!terminal.screen().alternate, "{drawn}");
    let screen = terminal.screen();
    assert!(
        screen.holds("nothing recorded: it still waits on the decision"),
        "{screen:#?}"
    );
    let status = yunta_in!(&checkout.repo, &checkout.home, &["status", run.as_str()]);
    assert!(stdout(&status).contains("needs you"), "{}", stdout(&status));
}

#[test]
fn review_answers_the_decision_the_way_resolve_gate_does() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let run = parked(&checkout);
    let mut terminal = yunta_on_terminal_sized!(
        &checkout.repo,
        &checkout.home,
        &["review", run.as_str()],
        WIDE
    );
    terminal.wait_on_screen("▸ the change", "review never opened the desk");
    terminal.keys("1");
    terminal.wait_on_screen("enter confirms", "a number never asked to be confirmed");
    terminal.keys("\r");
    let drawn = terminal.ended();
    assert_eq!(terminal.exit_code(), Some(0), "{drawn}");
    assert!(drawn.contains("resolved `approve`"), "{drawn}");
}

/// Off a terminal it is the plan as a page, and how to answer it.
#[test]
fn review_off_a_terminal_prints_the_plan_and_the_commands_that_answer_it() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let run = parked(&checkout);
    let review = yunta_in!(&checkout.repo, &checkout.home, &["review", run.as_str()]);
    assert!(review.status.success(), "{}", stderr(&review));
    let page = stdout(&review);
    assert!(page.contains("T001: Make it"), "{page}");
    assert!(page.contains("T002: Say it"), "{page}");
    assert!(page.contains("resolve-gate"), "{page}");
    assert!(page.contains("approve"), "{page}");
    assert!(page.contains("adjust"), "{page}");
}

/// It says what serves a person instead when there is no plan to answer.
#[test]
fn review_refuses_a_run_that_waits_on_no_plan_and_says_what_serves() {
    let root = tempfile::tempdir().unwrap();
    let without = REVIEWED.replace("    shows: [{ node: plan, kind: tasks }]\n", "");
    let checkout = checkout(root.path(), &without);
    let run = parked(&checkout);
    let review = yunta_in!(&checkout.repo, &checkout.home, &["review", run.as_str()]);
    assert!(!review.status.success());
    let said = stderr(&review);
    assert!(said.contains("shows no plan"), "{said}");
    assert!(said.contains("yunta status"), "{said}");

    let root = tempfile::tempdir().unwrap();
    let done = "name: done\nnodes:\n  - id: quick\n    kind: bash\n    run: \"true\"\n";
    let checkout = self::checkout(root.path(), done);
    let finished = yunta_in!(&checkout.repo, &checkout.home, &RUN);
    assert!(finished.status.success(), "{}", stderr(&finished));
    let run = run_id_from(&finished);
    let review = yunta_in!(&checkout.repo, &checkout.home, &["review", run.as_str()]);
    assert!(!review.status.success());
    let said = stderr(&review);
    assert!(said.contains("waits on no decision"), "{said}");
    assert!(said.contains("yunta status"), "{said}");
}

/// What is read of a parked run's next steps names the command, where the decision shows a
/// plan.
#[test]
fn a_run_parked_on_a_plan_offers_review() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path(), REVIEWED);
    let run = parked(&checkout);
    let status = yunta_in!(&checkout.repo, &checkout.home, &["status", run.as_str()]);
    let page = stdout(&status);
    assert!(page.contains("yunta review"), "{page}");
}
