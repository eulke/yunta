//! A task's card: its words in their order, and the code it writes, each
//! shape and each change once and under the change it is about, and each
//! shape said against what the tree held of it.

use surco::look::Look;
use surco::surface::{Markdown, Surface, Terminal};
use yunta_core::events::{InTree, Outside};
use yunta_core::shown::PlanReview;
use yunta_testkit_core::spell;

use super::documents::{global_packs, read, review, run};
use crate::plan::{document, Form};

/// What follows a card of the plan of global packs, on a terminal or in a
/// file: the next card, the next step or the proof, and in a file the next
/// section, a heading of the second level.
const AFTER_A_CARD: &[&str] = &[
    "\n    global-pack-",
    "\n  step ",
    "\n  proof",
    "\n#### ",
    "\n### ",
    "\n## ",
];

/// The card of `task` in `drawn`, up to what follows it.
fn card<'a>(drawn: &'a str, task: &str) -> &'a str {
    let start = drawn.find(&format!("{task}: ")).expect(drawn);
    let rest = &drawn[start + 1..];
    let end = AFTER_A_CARD
        .iter()
        .filter_map(|next| rest.find(next))
        .min()
        .map_or(drawn.len(), |at| start + 1 + at);
    &drawn[start..end]
}

/// `review` under review at a plain terminal 60 cells wide, where how a
/// card is laid out is read.
fn narrow(review: &PlanReview) -> String {
    let look = Look {
        width: Look::plain().width.within(60),
        ..Look::plain()
    };
    Terminal::on(look).draw(&document(review, &run(), Form::Review, &*spell()))
}

/// `review` under review as a file, where each row says its words on one
/// line.
fn written(review: &PlanReview) -> String {
    Markdown.draw(&document(review, &run(), Form::Review, &*spell()))
}

/// What is said under the code that `title` names in `drawn`: the last line
/// of its block, which a blank line closes.
fn footer(drawn: &str, title: &str) -> String {
    let block = &drawn[drawn.find(title).expect(title)..];
    let block = &block[..block.find("\n\n").unwrap_or(block.len())];
    block.lines().last().unwrap_or_default().trim().to_string()
}

/// What the tree held of the shape `name` of `review`, to change.
fn held<'a>(review: &'a mut PlanReview, name: &str) -> &'a mut InTree {
    let read = review.shapes_read.as_mut().expect("the tree was read");
    let shape = read.shapes.iter_mut().find(|shape| shape.name == name);
    &mut shape.expect(name).in_tree
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

/// A shape the tree holds and the plan changes says, after what its card
/// keeps and before what tests it, each file beyond the plan whose code
/// names it; a shape every other file naming which a task changes says
/// none.
#[test]
fn a_card_says_what_its_shapes_imply_after_what_it_keeps() {
    let review = global_packs();
    let drawn = narrow(&review);
    let catalog = card(&drawn, "global-pack-catalog");
    let row = |label: &str| {
        catalog
            .lines()
            .position(|line| line.trim_start().starts_with(label))
            .expect(label)
    };
    assert!(
        row("keeps") < row("implies") && row("implies") < row("tested by"),
        "{catalog}"
    );
    let file = written(&review);
    assert!(
        card(&file, "global-pack-catalog").contains(
            "- implies: resolve_workflow is named in 5 files no task changes:  \n  \
             crates/cli/src/commands/mod.rs  \n  \
             crates/engine/src/check/refs.rs  \n  \
             crates/engine/src/frozen/read.rs  \n  \
             crates/engine/src/lib.rs  \n  \
             crates/engine/tests/catalog.rs\n\
             - tested by: "
        ),
        "{file}"
    );
    let cli = card(&file, "global-pack-cli");
    assert!(!cli.contains("implies"), "{cli}");
}

/// Under each shape's code, what it is to the code that stands where it
/// goes and how long it is; under a change's own lines, how long they are.
#[test]
fn each_shape_says_what_it_is_to_the_tree_under_its_code() {
    let drawn = narrow(&global_packs());
    for (title, said) in [
        (":: PackInstallScope", "new ; 9 lines"),
        (":: installed_packs", "new ; 1 line"),
        (":: resolve_workflow", "modified ; 1 line"),
        (":: PackAction", "modified ; 9 lines"),
        (":: installed_layers", "1 line"),
        (":: PromptOrigin", "4 lines"),
        (":: pack_provenance", "1 line"),
        (":: pack_command", "1 line"),
        (":: add/update/remove/list", "4 lines"),
        (":: audit", "1 line"),
        ("\n      docs/global-install.md\n", "8 lines"),
    ] {
        assert_eq!(footer(&drawn, title), said, "under `{title}`:\n{drawn}");
    }
}

/// A shape whose code reads as what stands where it goes, word for word,
/// says so and implies nothing: whoever names it reads it as before.
#[test]
fn a_shape_kept_as_it_reads_implies_nothing() {
    let mut review = global_packs();
    let planned = review
        .plan
        .shapes
        .iter()
        .find(|shape| shape.name == "resolve_workflow")
        .expect("the plan writes resolve_workflow")
        .code
        .clone();
    let InTree::There { code, .. } = held(&mut review, "resolve_workflow") else {
        panic!("the tree holds resolve_workflow");
    };
    *code = Some(planned);
    let drawn = narrow(&review);
    let catalog = card(&drawn, "global-pack-catalog");
    assert_eq!(
        footer(catalog, ":: resolve_workflow"),
        "unchanged ; 1 line",
        "{catalog}"
    );
    assert!(!catalog.contains("implies"), "{catalog}");
}

/// A name so many files carry that it is a word of the language more than
/// a shape of the plan is said in one row, with how many carry it.
#[test]
fn a_name_too_common_to_follow_is_said_without_its_files() {
    let mut review = global_packs();
    let InTree::There { outside, .. } = held(&mut review, "resolve_workflow") else {
        panic!("the tree holds resolve_workflow");
    };
    *outside = Outside::Common { files: 147 };
    let file = written(&review);
    assert!(
        card(&file, "global-pack-catalog").contains(
            "- implies: resolve_workflow is named in 147 files, too many to follow\n\
             - tested by: "
        ),
        "{file}"
    );
}

/// A path a shape implies reads as the path in a file, where Markdown would
/// read the underscores around `init` as bold, and as it is on a terminal.
#[test]
fn an_implied_path_reads_as_the_path_in_a_file() {
    let mut review = global_packs();
    let InTree::There { outside, .. } = held(&mut review, "resolve_workflow") else {
        panic!("the tree holds resolve_workflow");
    };
    *outside = Outside::Files {
        paths: vec!["pkg/__init__.py".to_string()],
    };
    let file = written(&review);
    assert!(
        card(&file, "global-pack-catalog").contains(concat!(
            "- implies: resolve_workflow is named in 1 file no task changes:  \n",
            r"  pkg/\_\_init\_\_.py",
            "\n- tested by: "
        )),
        "{file}"
    );
    let drawn = narrow(&review);
    let catalog = card(&drawn, "global-pack-catalog");
    assert!(
        catalog.lines().any(|line| line.trim() == "pkg/__init__.py"),
        "{catalog}"
    );
}

/// A shape its file names in code whose items are not read is said to
/// exist there, marked nowhere, and still implies who else names it.
#[test]
fn a_shape_whose_items_are_not_read_is_said_to_exist() {
    let mut review = global_packs();
    let InTree::There { code, .. } = held(&mut review, "resolve_workflow") else {
        panic!("the tree holds resolve_workflow");
    };
    *code = None;
    let drawn = narrow(&review);
    let catalog = card(&drawn, "global-pack-catalog");
    assert_eq!(
        footer(catalog, ":: resolve_workflow"),
        "exists, not compared ; 1 line",
        "{catalog}"
    );
    assert!(!catalog.contains("{+"), "nothing is marked:\n{catalog}");
    assert!(
        catalog
            .lines()
            .any(|line| line.trim_start().starts_with("implies")),
        "{catalog}"
    );
}

/// A plan whose shapes no tree was read for says how long each code is,
/// and nothing of what it is to the tree or what it implies.
#[test]
fn a_plan_read_against_no_tree_says_only_its_sizes() {
    let mut review = global_packs();
    review.shapes_read = None;
    let drawn = narrow(&review);
    for (title, said) in [
        (":: PackInstallScope", "9 lines"),
        (":: installed_packs", "1 line"),
        (":: resolve_workflow", "1 line"),
        (":: PackAction", "9 lines"),
    ] {
        assert_eq!(footer(&drawn, title), said, "under `{title}`:\n{drawn}");
    }
    assert!(!drawn.contains("implies"), "{drawn}");
    assert!(!drawn.contains("{+"), "{drawn}");
}

#[test]
fn a_decision_is_numbered_and_says_what_it_chose_instead_of_what() {
    let drawn = narrow(&global_packs());
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
