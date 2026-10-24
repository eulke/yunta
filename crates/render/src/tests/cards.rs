//! A task's card: its words in their order, and the code it writes, each
//! shape and each change once and under the change it is about.

use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_testkit_core::spell;

use super::documents::{global_packs, read, review, run};
use crate::plan::{document, Form};

/// The card of `task` in `drawn`, up to the card after it.
fn card<'a>(drawn: &'a str, task: &str) -> &'a str {
    let start = drawn.find(&format!("{task}: ")).expect(drawn);
    let rest = &drawn[start + 1..];
    let end = rest
        .find(": ")
        .and_then(|_| {
            rest.find("\n  global-pack-")
                .or_else(|| rest.find("\nstep "))
        })
        .map_or(drawn.len(), |at| start + 1 + at);
    &drawn[start..end]
}

/// A change that carries its code shows it, beside what it is about.
#[test]
fn a_change_shows_its_own_code() {
    let mut review = review();
    review.plan.tasks[1].changes[0].code = Some("pub fn bye(&self) -> String;".to_string());
    let drawn = read(&review, Form::Review);
    let card = &drawn[drawn.find("farewell: Say goodbye").unwrap()..];
    assert!(card.contains("| pub fn bye(&self) -> String;"), "{card}");
    assert!(card.contains("the goodbye, beside the greeting"), "{card}");
}

#[test]
fn a_shape_reads_before_the_lines_a_change_adds_to_its_file() {
    let mut review = review();
    review.plan.tasks[0].changes[0].code = Some("pub fn hello(&self) -> String;".to_string());
    let drawn = read(&review, Form::Review);
    let card = &drawn
        [drawn.find("greet: Greet by name").unwrap()..drawn.find("farewell: Say goodbye").unwrap()];
    let shape = card
        .find("| pub struct Greeter {")
        .expect("the shape, whole");
    let line = card
        .find("| pub fn hello(&self) -> String;")
        .expect("its line");
    assert!(shape < line, "{card}");
    assert_eq!(card.matches("| pub struct Greeter {").count(), 1, "{card}");
}

/// Each shape is drawn under the change that is about it, whatever order
/// the task names its changes in.
#[test]
fn a_shape_is_drawn_under_the_change_it_is_about() {
    let mut review = global_packs();
    review.plan.tasks[0].changes.reverse();
    let drawn = read(&review, Form::Review);
    let storage = card(&drawn, "global-pack-storage");
    for (shape, what) in [
        ("PackInstallScope", "scope enum and root selection API"),
        (
            "installed_packs",
            "scope-aware lock location and pack enumeration",
        ),
    ] {
        let title = storage.find(&format!(":: {shape}")).expect(storage);
        let caption = &storage[title..].lines().nth(1).unwrap_or_default();
        assert!(
            caption.contains(what),
            "`{shape}` under `{what}`:\n{storage}"
        );
    }
}

/// A change with no lines of its own about one shape of a file leaves the
/// other shape of that file drawn too.
#[test]
fn every_shape_a_task_builds_is_drawn_once() {
    let mut review = global_packs();
    let storage = &mut review.plan.tasks[0];
    storage
        .changes
        .retain(|change| change.at.ends_with("installed_packs"));
    storage.changes[0].code = None;
    let drawn = read(&review, Form::Review);
    let storage = card(&drawn, "global-pack-storage");
    for shape in ["pub enum PackInstallScope {", "pub fn installed_packs("] {
        assert_eq!(
            storage.matches(shape).count(),
            1,
            "`{shape}` once:\n{storage}"
        );
    }
}

#[test]
fn a_decision_is_numbered_and_says_what_it_chose_instead_of_what() {
    let look = Look {
        width: Look::plain().width.within(60),
        ..Look::plain()
    };
    let drawn =
        Terminal::on(look).draw(&document(&global_packs(), &run(), Form::Review, &*spell()));
    let lines: Vec<&str> = drawn.lines().collect();
    let first = lines
        .iter()
        .position(|line| {
            line.trim_start()
                .starts_with("1  What scope and persistence")
        })
        .expect(&drawn);
    let question = lines[first].find("What").unwrap();
    let goes_on = lines[first + 1];
    assert_eq!(
        goes_on.len() - goes_on.trim_start().len(),
        question,
        "{drawn}"
    );
    let row = |label: &str| {
        lines[first..]
            .iter()
            .position(|line| line.trim_start().starts_with(label))
            .expect(label)
    };
    assert!(row("chose") < row("instead of") && row("instead of") < row("because"));
}

#[test]
fn an_answered_decision_is_yours_and_the_rest_were_decided_for_you() {
    let drawn = read(&review(), Form::Review);
    let ours = drawn
        .find("decided for you")
        .expect("what the plan decided");
    let yours = drawn.find("you answered").expect("what you answered");
    assert!(ours < yours, "{drawn}");
    assert!(drawn.contains("you said     hello"), "{drawn}");
    assert!(
        !drawn[ours..yours].contains("Which word greets?"),
        "{drawn}"
    );
}

#[test]
fn a_card_names_the_shapes_its_task_builds_on_and_what_tests_it() {
    let mut review = review();
    review.plan.tasks[0].uses = vec!["Greeter".to_string()];
    let drawn = read(&review, Form::Review);
    let greet = drawn.find("greet: Greet by name").expect(&drawn);
    let farewell = drawn.find("farewell: Say goodbye").expect(&drawn);
    assert!(!drawn[greet..farewell].contains("builds on"), "{drawn}");
    assert!(
        drawn[farewell..].contains("builds on    Greeter (from greet)"),
        "{drawn}"
    );
    assert!(
        drawn[greet..farewell].contains("tested by    the greeting names the person"),
        "{drawn}"
    );
    assert!(
        !drawn[greet..farewell].contains("tests/greet.rs"),
        "a test the spec wrote is no change of the task's:\n{drawn}"
    );
}
