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
use crate::prompt::decision_card::Offered;
use crate::prompt::{Answerable, PickedOption, Press, Stroke};

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

/// The option `name`, which says `said`, asking `asks` of words where it asks any.
fn option(name: &str, said: &str, asks: Option<&str>) -> Offered {
    Offered {
        id: name.parse().expect("an option's id"),
        row: ChoiceRow {
            name: name.to_string(),
            said: Some(said.to_string()),
            cost: None,
            key: None,
        },
        asks: asks.map(str::to_string),
    }
}

/// `option` chosen at the desk, with no words.
fn chose(option: &str) -> Press<Left> {
    Press::Answer(Left::Chose(PickedOption {
        option: option.parse().expect("an option's id"),
        words: None,
        folded: false,
    }))
}

fn desk(review: &PlanReview, leaving: Leaving) -> PlanDesk {
    PlanDesk::new(
        Reading::of(review.clone()),
        Asked {
            question: "Plan registered. Approve?".to_string(),
            node: Some("approve-plan".to_string()),
            run: Some("EKPBGE".to_string()),
            workflow: Some("harvest".to_string()),
        },
        vec![
            option("approve", "On to `ship`: resolves this gate", None),
            option(
                "adjust",
                "Back to `plan`: sends what you say",
                Some("what should change?"),
            ),
        ],
        leaving,
    )
}

pub(crate) fn look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(COLUMNS),
        motion: surco::motion::Motion::Moves,
    }
}

/// The desk as a terminal holds it, handed back what was laid out of its pane as the
/// terminal's loop hands it back.
fn rows(desk: &mut PlanDesk) -> Vec<String> {
    let held = surco::surface::Terminal::on(look()).held(&desk.block(&look()), COLUMNS, ROWS);
    if let Some(laid) = held.laid {
        desk.laid(laid);
    }
    held.lines.iter().map(|line| line.text()).collect()
}

fn holds(desk: &mut PlanDesk, needle: &str) -> bool {
    rows(desk).iter().any(|row| row.contains(needle))
}

fn press(desk: &mut PlanDesk, keys: &str) {
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
    assert!(holds(&mut desk, "prints `hello, ana`"));
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
        .map(|at| option(&format!("o{at}"), "does something", None))
        .collect();
    assert!(holds(&mut many, "9 o9"), "{:#?}", rows(&mut many));
    assert!(!holds(&mut many, "10 o10"), "{:#?}", rows(&mut many));
    let drawn = rows(&mut many);
    let foot: Vec<&str> = drawn.iter().map(|row| row.trim()).collect();
    let foot = foot.join(" ");
    assert!(foot.contains("2 more options on the card"), "{foot}");
    press(&mut many, "9");
    assert_eq!(many.press(Stroke::Enter), chose("o9"));
}

/// A run's screen opens the desk on the plan's first task, what the plan says of itself a
/// key back.
#[test]
fn on_a_runs_screen_it_opens_on_the_plans_first_task() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard).on_its_first_task();
    assert!(holds(&mut desk, "▸ greet"), "{:#?}", rows(&mut desk));
    press(&mut desk, "k");
    assert!(holds(&mut desk, "▸ the change"), "{:#?}", rows(&mut desk));
}

/// Held beside keys of its own, the desk takes only the keys it reads by: a number that
/// names an option, the walk, `p`, `d` and the keys that scroll; `enter` and `esc` only while
/// a number waits to be confirmed; every key while words are typed at its foot, the only time
/// what is typed or pasted there is words. The screen's legend says them.
#[test]
fn on_a_screen_it_takes_only_the_keys_it_reads_by() {
    let review = review();
    let mut desk = desk(&review, Leaving::TheCommand(Escape::LeavesWaiting)).on_its_first_task();
    let reads_by = [
        Stroke::Insert('1'),
        Stroke::Insert('2'),
        Stroke::Insert('j'),
        Stroke::Insert('k'),
        Stroke::Tab,
        Stroke::BackTab,
        Stroke::Insert('p'),
        Stroke::Insert('d'),
        Stroke::Down,
        Stroke::PageUp,
    ];
    let the_screens = [
        Stroke::Insert('3'),
        Stroke::Enter,
        Stroke::Decline,
        Stroke::Interrupt,
        Stroke::Insert('q'),
        Stroke::Insert('s'),
    ];
    assert!(reads_by.iter().all(|stroke| desk.takes(*stroke)) && !desk.typing());
    assert!(!the_screens.iter().any(|stroke| desk.takes(*stroke)));
    assert_eq!(
        desk.legend_on_a_screen().spelled(Glyphs::Unicode),
        "1 2 answer · tab next place · ↑↓ scroll · p proof · d decided for you · esc leave it \
         waiting"
    );
    press(&mut desk, "1");
    assert!(desk.takes(Stroke::Enter) && desk.takes(Stroke::Decline));
    assert_eq!(
        desk.legend_on_a_screen().spelled(Glyphs::Unicode),
        "enter confirm · esc go back"
    );
    press(&mut desk, "2");
    assert!(
        !desk.typing(),
        "a number waiting to be confirmed is no words"
    );
    assert_eq!(desk.press(Stroke::Enter), Press::Stay);
    assert!(desk.typing());
    assert!(the_screens.iter().all(|stroke| desk.takes(*stroke)));
    assert_eq!(
        desk.legend_on_a_screen().spelled(Glyphs::Unicode),
        "enter send it · esc go back"
    );
}

/// `p` turns a task's card over, and `j` walks the proofs while it is. The suite every task
/// keeps passing is the whole plan's proof's to say.
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
    assert!(holds(&mut desk, "p proof"), "{:#?}", rows(&mut desk));
    press(&mut desk, "j");
    assert!(
        holds(&mut desk, "greet, its proof"),
        "{:#?}",
        rows(&mut desk)
    );
    assert!(!holds(&mut desk, "the suite"), "{:#?}", rows(&mut desk));
    press(&mut desk, "j");
    assert!(
        holds(&mut desk, "farewell, its proof"),
        "{:#?}",
        rows(&mut desk)
    );
    press(&mut desk, "p");
    assert!(
        holds(&mut desk, "prints a goodbye"),
        "{:#?}",
        rows(&mut desk)
    );
}

/// `d` opens what the plan decided, every decision whole, the rail still pointing at the
/// place stood on; what it risks the rail counts. `j` walks on to the next place and shows
/// it, and `d` goes back.
#[test]
fn d_shows_every_decision_and_j_walks_on_to_the_next_place() {
    let review = review();
    let mut desk = desk(&review, Leaving::ToTheCard);
    press(&mut desk, "d");
    assert!(holds(&mut desk, "2 choices"), "{:#?}", rows(&mut desk));
    assert!(
        holds(&mut desk, "What does it say"),
        "{:#?}",
        rows(&mut desk)
    );
    assert!(holds(&mut desk, "It greets"), "{:#?}", rows(&mut desk));
    assert!(holds(&mut desk, "It says"), "{:#?}", rows(&mut desk));
    assert!(
        !holds(&mut desk, "breaks the line"),
        "its risks are counted on the rail: {:#?}",
        rows(&mut desk)
    );
    assert!(holds(&mut desk, "▸ the change"), "{:#?}", rows(&mut desk));
    assert!(holds(&mut desk, "tab next place"), "{:#?}", rows(&mut desk));
    press(&mut desk, "d");
    assert!(
        holds(&mut desk, "The program says nothing today"),
        "and d goes back: {:#?}",
        rows(&mut desk)
    );
    press(&mut desk, "dj");
    assert!(holds(&mut desk, "▸ greet"), "{:#?}", rows(&mut desk));
    assert!(
        holds(&mut desk, "prints `hello, ana`"),
        "{:#?}",
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
    assert!(holds(&mut desk, "enter confirm"), "{:#?}", rows(&mut desk));
    press(&mut desk, "2");
    assert!(
        holds(&mut desk, "Back to `plan`"),
        "another number changes it"
    );
    press(&mut desk, "1");
    assert_eq!(desk.press(Stroke::Enter), chose("approve"));
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
