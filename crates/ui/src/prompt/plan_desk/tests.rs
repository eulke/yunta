//! What the plan's desk does with each key, with no terminal: a stroke goes in and what
//! it asks of the prompt comes out.

use surco::components::ChoiceRow;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::paint::Paint;
use yunta_core::shown::PlanReview;
use yunta_core::TasksFile;
use yunta_render::page::desk::Reading;
use yunta_render::prompt::Escape;

use super::{Asked, Leaving, Left, PlanDesk};
use crate::prompt::{Answerable, Press, Stroke};

/// A terminal with room for a rail, in the rows these tests read.
const COLUMNS: usize = 110;
const ROWS: usize = 30;

const PLAN: &str = r#"
summary: Greet a person by name.
description: The program says nothing today.
decisions:
  - id: default-name
    question: What does it say when no name is given?
    choice: It greets `world`.
    alternatives: [Refuse to run]
    why: A program run with no arguments should still say something.
  - id: greeting-word
    question: Which word greets?
    choice: It says `hello`.
risks:
  - A name with a newline in it breaks the line it is printed on.
tasks:
  - id: greet
    title: Greet by name
    scope: [src/greet.rs]
    criteria:
      - { cmd: cargo test --test greet, proves: the greeting names the person }
    changes:
      - at: src/greet.rs
        what: the greeting
        code: |-
          fn line1() {}
          fn line2() {}
          fn line3() {}
          fn line4() {}
          fn line5() {}
          fn line6() {}
          fn line7() {}
          fn line8() {}
          fn line9() {}
          fn line10() {}
          fn line11() {}
          fn line12() {}
          fn line13() {}
          fn line14() {}
          fn line15() {}
          fn line16() {}
          fn line17() {}
          fn line18() {}
          fn line19() {}
          fn line20() {}
          fn line21() {}
          fn line22() {}
          fn line23() {}
          fn line24() {}
          fn line25() {}
    outcome: "`hello ana` prints `hello, ana`."
  - id: farewell
    title: Say goodbye
    scope: [src/bye.rs]
    depends_on: [greet]
    criteria:
      - { cmd: cargo test --test bye, proves: the program also says goodbye }
    changes:
      - { at: src/bye.rs, what: the goodbye }
    outcome: "`hello ana` prints a goodbye."
"#;

pub(crate) fn review() -> PlanReview {
    PlanReview {
        plan: yunta_core::yaml::parse::<TasksFile>(PLAN).unwrap(),
        departed: Vec::new(),
        spec: None,
        spec_node: None,
        asked: Some("greet people".to_string()),
        held: vec!["cargo test".to_string()],
        tasks: Vec::new(),
        handed_over: None,
        answered: Vec::new(),
        shapes_read: None,
    }
}

fn option(name: &str, said: &str) -> ChoiceRow {
    ChoiceRow {
        name: name.to_string(),
        said: Some(said.to_string()),
        cost: None,
    }
}

fn desk(review: &PlanReview, leaving: Leaving) -> PlanDesk<'_> {
    PlanDesk::new(
        Reading::of(review, Vec::new()),
        Asked {
            question: "Plan registered. Approve?".to_string(),
            node: Some("approve-plan".to_string()),
            run: Some("EKPBGE".to_string()),
            called: Some("harvest (standard)".to_string()),
        },
        vec![
            option("approve", "On to `ship`: resolves this gate"),
            option("adjust", "Back to `plan`: sends what you say"),
        ],
        leaving,
    )
}

pub(crate) fn look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(COLUMNS),
    }
}

/// The desk as a terminal holds it, handed back what was laid out of its pane as the
/// terminal's loop hands it back.
fn rows(desk: &mut PlanDesk<'_>) -> Vec<String> {
    let held = surco::surface::Terminal::on(look()).held(&desk.block(&look()), COLUMNS, ROWS);
    if let Some(pane) = held.pane {
        desk.laid(pane);
    }
    held.lines.iter().map(|line| line.text()).collect()
}

fn holds(desk: &mut PlanDesk<'_>, needle: &str) -> bool {
    rows(desk).iter().any(|row| row.contains(needle))
}

fn press(desk: &mut PlanDesk<'_>, keys: &str) {
    for key in keys.chars() {
        assert_eq!(desk.press(Stroke::Insert(key)), Press::Stay, "{key}");
    }
}

/// `j` and `k` walk the plan, and go around at its ends: first what it says of itself,
/// then each task.
#[test]
fn j_and_k_walk_the_stops_and_go_around() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard);
    assert!(holds(&mut desk, "▸ the change"), "{:#?}", rows(&mut desk));
    assert!(holds(&mut desk, "The program says nothing today"));
    press(&mut desk, "j");
    assert!(holds(&mut desk, "▸ greet"), "{:#?}", rows(&mut desk));
    assert!(holds(&mut desk, "Greet by name"));
    assert!(holds(&mut desk, "step 1 of 2"), "{:#?}", rows(&mut desk));
    press(&mut desk, "j");
    assert!(holds(&mut desk, "▸ farewell"));
    assert!(holds(&mut desk, "step 2 of 2"));
    press(&mut desk, "j");
    assert!(holds(&mut desk, "▸ the change"), "it goes around");
    press(&mut desk, "k");
    assert!(holds(&mut desk, "▸ farewell"), "and back around");
}

/// The arrows move what is read, and moving to another task starts at its top.
#[test]
fn the_arrows_scroll_the_pane_and_a_new_task_starts_at_its_top() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard);
    press(&mut desk, "j");
    assert!(holds(&mut desk, "fn line1()"), "{:#?}", rows(&mut desk));
    for _ in 0..12 {
        assert_eq!(desk.press(Stroke::Down), Press::Stay);
        rows(&mut desk);
    }
    assert!(!holds(&mut desk, "fn line1()"), "{:#?}", rows(&mut desk));
    assert_eq!(desk.press(Stroke::Up), Press::Stay);
    press(&mut desk, "j");
    press(&mut desk, "k");
    assert!(holds(&mut desk, "fn line1()"), "{:#?}", rows(&mut desk));
}

#[test]
fn a_page_key_moves_by_what_is_in_sight_and_stops_at_the_ends() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard);
    press(&mut desk, "j");
    rows(&mut desk);
    assert_eq!(desk.press(Stroke::PageDown), Press::Stay);
    assert!(holds(&mut desk, "fn line25()"), "{:#?}", rows(&mut desk));
    for _ in 0..5 {
        desk.press(Stroke::PageDown);
        rows(&mut desk);
    }
    assert!(holds(&mut desk, "fn line25()"), "{:#?}", rows(&mut desk));
    assert_eq!(desk.press(Stroke::Home), Press::Stay);
    assert!(holds(&mut desk, "fn line1()"), "{:#?}", rows(&mut desk));
    assert_eq!(desk.press(Stroke::End), Press::Stay);
    assert!(holds(&mut desk, "fn line25()"), "{:#?}", rows(&mut desk));
    assert_eq!(desk.press(Stroke::PageUp), Press::Stay);
    press(&mut desk, " ");
    assert!(
        holds(&mut desk, "fn line25()"),
        "space pages down: {:#?}",
        rows(&mut desk)
    );
}

/// A person who pages through what is read reads every line of it: the row that counts what
/// is below takes a line's place, and a page moves by what was in sight.
#[test]
fn paging_through_a_pane_shows_every_line() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard);
    press(&mut desk, "j");
    let mut seen: Vec<String> = rows(&mut desk);
    for _ in 0..12 {
        desk.press(Stroke::PageDown);
        seen.extend(rows(&mut desk));
    }
    for line in 1..=25 {
        let wanted = format!("fn line{line}()");
        assert!(
            seen.iter().any(|row| row.contains(&wanted)),
            "{wanted} never came into sight"
        );
    }
}

/// A number takes one key, so a decision with more options than digits names the first nine
/// and says where the rest are.
#[test]
fn a_tenth_option_is_on_the_card() {
    let review = review();
    let mut many = desk(&review, Leaving::ToTheCard);
    many.offered = (1..=11)
        .map(|at| option(&format!("o{at}"), "does something"))
        .collect();
    assert!(holds(&mut many, "9 o9"), "{:#?}", rows(&mut many));
    assert!(!holds(&mut many, "10 o10"), "{:#?}", rows(&mut many));
    assert!(
        holds(&mut many, "2 more options on the card"),
        "{:#?}",
        rows(&mut many)
    );
    press(&mut many, "9");
    assert_eq!(many.press(Stroke::Enter), Press::Answer(Left::Picked(8)));
}

/// `p` turns a task's card over, and `j` walks the proofs while it is.
#[test]
fn p_shows_what_proves_the_task_and_walks_with_it() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard);
    assert!(holds(&mut desk, "p proof"), "{:#?}", rows(&mut desk));
    press(&mut desk, "p");
    assert!(
        holds(&mut desk, "proof · the whole plan"),
        "{:#?}",
        rows(&mut desk)
    );
    assert!(holds(&mut desk, "the suite"), "{:#?}", rows(&mut desk));
    assert!(holds(&mut desk, "p the card"), "{:#?}", rows(&mut desk));
    press(&mut desk, "j");
    assert!(holds(&mut desk, "proof · greet"), "{:#?}", rows(&mut desk));
    assert!(holds(&mut desk, "the suite"), "{:#?}", rows(&mut desk));
    press(&mut desk, "j");
    assert!(
        holds(&mut desk, "proof · farewell"),
        "{:#?}",
        rows(&mut desk)
    );
    press(&mut desk, "p");
    assert!(holds(&mut desk, "Say goodbye"), "{:#?}", rows(&mut desk));
}

/// `d` opens what was decided, one decision open at a time, with `j` and `k` between them.
#[test]
fn d_shows_what_was_decided_and_j_walks_the_choices() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard);
    press(&mut desk, "d");
    assert!(
        holds(&mut desk, "decided for you"),
        "{:#?}",
        rows(&mut desk)
    );
    assert!(
        holds(&mut desk, "▸ 1 What does it say"),
        "{:#?}",
        rows(&mut desk)
    );
    assert!(holds(&mut desk, "It greets"), "{:#?}", rows(&mut desk));
    assert!(!holds(&mut desk, "It says"), "{:#?}", rows(&mut desk));
    assert!(
        holds(&mut desk, "breaks the line"),
        "its risks: {:#?}",
        rows(&mut desk)
    );
    assert!(
        !rows(&mut desk)
            .iter()
            .any(|row| row.contains("▸ the change")),
        "the rail points at no task: {:#?}",
        rows(&mut desk)
    );
    assert!(holds(&mut desk, "j k choice"), "{:#?}", rows(&mut desk));
    press(&mut desk, "j");
    assert!(
        holds(&mut desk, "▸ 2 Which word greets"),
        "{:#?}",
        rows(&mut desk)
    );
    assert!(holds(&mut desk, "It says"), "{:#?}", rows(&mut desk));
    assert!(!holds(&mut desk, "It greets"), "{:#?}", rows(&mut desk));
    press(&mut desk, "j");
    assert!(holds(&mut desk, "▸ 1 What does it say"), "it goes around");
    press(&mut desk, "d");
    assert!(
        holds(&mut desk, "▸ the change"),
        "and d goes back: {:#?}",
        rows(&mut desk)
    );
}

/// A number says what it would do and asks: reading a plan is no time for one key to
/// approve it.
#[test]
fn a_number_asks_and_enter_confirms() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard);
    assert!(
        holds(&mut desk, "1 approve   2 adjust"),
        "{:#?}",
        rows(&mut desk)
    );
    press(&mut desk, "1");
    assert!(holds(&mut desk, "On to `ship`"), "{:#?}", rows(&mut desk));
    assert!(holds(&mut desk, "enter confirms"), "{:#?}", rows(&mut desk));
    press(&mut desk, "2");
    assert!(
        holds(&mut desk, "Back to `plan`"),
        "another number changes it"
    );
    assert_eq!(desk.press(Stroke::Enter), Press::Answer(Left::Picked(1)));
}

#[test]
fn esc_takes_a_number_back_and_nothing_else_answers() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard);
    press(&mut desk, "1");
    assert_eq!(desk.press(Stroke::Decline), Press::Stay);
    assert!(
        holds(&mut desk, "1 approve   2 adjust"),
        "{:#?}",
        rows(&mut desk)
    );
    assert_eq!(
        desk.press(Stroke::Enter),
        Press::Stay,
        "enter with no number"
    );
    press(&mut desk, "9");
    assert!(holds(&mut desk, "1 approve   2 adjust"), "no such option");
    press(&mut desk, "0");
    assert!(holds(&mut desk, "1 approve   2 adjust"), "no such option");
}

#[test]
fn esc_and_q_leave_and_ctrl_c_stops_from_anywhere() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard);
    assert_eq!(desk.press(Stroke::Decline), Press::Answer(Left::Back));
    assert_eq!(desk.press(Stroke::Insert('q')), Press::Answer(Left::Back));
    assert_eq!(
        desk.press(Stroke::Interrupt),
        Press::Answer(Left::Interrupted)
    );
    press(&mut desk, "1");
    assert_eq!(
        desk.press(Stroke::Interrupt),
        Press::Answer(Left::Interrupted),
        "a number being confirmed does not hold it"
    );
}

/// What Escape does is said: back to the card it came from, or out of the command that
/// opened it, with the run left as it is.
#[test]
fn the_keys_say_where_esc_goes() {
    let review = review();
    let mut from_the_card = desk(&review, Leaving::ToTheCard);
    assert!(
        holds(&mut from_the_card, "esc back"),
        "{:#?}",
        rows(&mut from_the_card)
    );
    let mut opened = desk(&review, Leaving::TheCommand(Escape::LeavesWaiting));
    assert!(
        holds(&mut opened, &Escape::LeavesWaiting.said()),
        "{:#?}",
        rows(&mut opened)
    );
}

/// Nothing a person types here is words, so a paste answers nothing.
#[test]
fn it_is_never_typing() {
    let review = review();
    assert!(!desk(&review, Leaving::ToTheCard).typing());
}
