//! A plan as its desk reads it: every place a person can stand on, what each shows, and
//! what the rail says of the whole, from the plan the rest of these tests read.

use surco::components::{DeskFoot, FootRow, RailGroup, RailRow};
use surco::doc::Doc;
use surco::glyphs::Glyphs;
use surco::golden::assert_golden;
use surco::line::Line;
use surco::look::{Look, Width};
use surco::paint::Paint;
use yunta_core::events::{InTree, Outside, ShapesReadPayload};
use yunta_core::shown::PlanReview;

use super::documents::{global_packs, goldens, review as greeting};
use crate::plan::{about, AlsoShown, Reading, Spot, View};

fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::whole(cells),
    }
}

/// What a person spot on `spot` reads in the pane of a terminal 80 cells wide.
fn read(reading: &Reading<'_>, spot: &Spot) -> Vec<String> {
    reading
        .pane_lines(spot, &look(80))
        .iter()
        .map(Line::text)
        .collect()
}

fn holds(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|row| row.contains(needle))
}

/// The rail as a person spot on `at` reads it.
fn rail(reading: &Reading<'_>, at: usize) -> Vec<RailGroup> {
    reading.rail(&Spot {
        at,
        view: View::Stop,
        top: 0,
        choice: 0,
    })
}

/// A rail's groups by what each opens with, and each stop as `lead name`.
fn stops(groups: &[RailGroup]) -> Vec<(String, String, bool)> {
    groups
        .iter()
        .flat_map(|group| &group.rows)
        .filter_map(|row| match row {
            RailRow::Stop {
                lead,
                name,
                pointed,
            } => Some((lead.clone(), name.clone(), *pointed)),
            _ => None,
        })
        .collect()
}

fn group<'a>(groups: &'a [RailGroup], title: &str) -> Option<&'a RailGroup> {
    groups
        .iter()
        .find(|group| group.title.as_deref() == Some(title))
}

/// A person starts on what the plan says of itself, and then walks its tasks, step by
/// step: a step's number stands on its first task.
#[test]
fn the_stops_run_from_what_the_plan_says_through_each_task_in_its_step() {
    let review = global_packs();
    let reading = Reading::of(&review, Vec::new());
    assert_eq!(reading.stops(), 5);
    let shown = stops(&rail(&reading, 2));
    assert_eq!(
        shown,
        [
            (String::new(), "the change".to_string(), false),
            ("1".to_string(), "global-pack-storage".to_string(), false),
            (String::new(), "global-pack-catalog".to_string(), true),
            ("2".to_string(), "global-pack-cli".to_string(), false),
            ("3".to_string(), "global-pack-docs".to_string(), false),
        ]
    );
}

/// A plan with nothing to say of itself opens on its first task.
#[test]
fn a_plan_with_no_description_or_design_opens_on_its_first_task() {
    let mut review = greeting();
    review.plan.description = None;
    review.plan.design = None;
    let reading = Reading::of(&review, Vec::new());
    assert_eq!(reading.stops(), 2);
    assert_eq!(stops(&rail(&reading, 0))[0].1, "greet");
    assert!(stops(&rail(&reading, 0))[0].2);
}

/// What else the decision shows is one more stop, in its own group.
#[test]
fn what_else_a_decision_shows_is_a_stop_too() {
    let review = greeting();
    let also = vec![AlsoShown {
        name: "the brief".to_string(),
        doc: Doc::new().with(surco::components::Gist::new("what was asked")),
        leads: false,
    }];
    let reading = Reading::of(&review, also);
    assert_eq!(reading.stops(), 4);
    let groups = rail(&reading, 3);
    let also = group(&groups, "also shown").expect("its group");
    assert!(
        matches!(&also.rows[0], RailRow::Stop { name, pointed: true, .. } if name == "the brief"),
    );
    let spot = Spot {
        at: 3,
        view: View::Stop,
        top: 0,
        choice: 0,
    };
    assert!(holds(&read(&reading, &spot), "what was asked"));
    assert!(
        !reading.has_proof(3),
        "only a task has a proof to turn over to"
    );
}

/// What the decision lists before its plan is read before it.
#[test]
fn what_a_decision_shows_before_its_plan_is_read_before_it() {
    let review = greeting();
    let also = vec![AlsoShown {
        name: "the brief".to_string(),
        doc: Doc::new().with(surco::components::Gist::new("what was asked")),
        leads: true,
    }];
    let reading = Reading::of(&review, also);
    let shown = stops(&rail(&reading, 0));
    assert_eq!(shown[0], (String::new(), "the brief".to_string(), true));
    assert_eq!(shown[1].1, "the change");
    assert_eq!(shown[2].1, "greet");
    let spot = Spot {
        at: 0,
        view: View::Stop,
        top: 0,
        choice: 0,
    };
    assert!(holds(&read(&reading, &spot), "what was asked"));
}

/// The order is what each task waits on, named without what every name repeats.
#[test]
fn the_rail_says_the_order_and_who_changes_what_by_short_names() {
    let review = global_packs();
    let reading = Reading::of(&review, Vec::new());
    let groups = rail(&reading, 1);
    assert!(
        matches!(
            group(&groups, "order").map(|group| &group.rows[..]),
            Some([RailRow::Order(_)])
        ),
        "its order"
    );
    let impact = group(&groups, "impact").expect("its impact");
    let rows: Vec<(&str, &str)> = impact
        .rows
        .iter()
        .map(|row| match row {
            RailRow::Pair { left, right } => (left.as_str(), right.as_str()),
            _ => panic!("impact is paths and tasks"),
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("crates/cli/src/pack.rs", "storage"),
            ("crates/engine/src/catalog.rs", "catalog"),
            ("crates/engine/src/frozen/", "catalog"),
            ("crates/cli/src/cli/", "cli"),
            ("crates/cli/src/commands/", "cli"),
            ("docs/global-install.md", "docs"),
        ],
        "a directory stands for what a task changes more than one file of"
    );
}

#[test]
fn a_plan_of_one_task_has_no_order_and_keeps_its_whole_name() {
    let mut review = greeting();
    review.plan.tasks.truncate(1);
    let reading = Reading::of(&review, Vec::new());
    let groups = rail(&reading, 1);
    assert!(group(&groups, "order").is_none());
    let impact = group(&groups, "impact").expect("its impact");
    assert!(matches!(&impact.rows[0], RailRow::Pair { right, .. } if right == "greet"),);
}

/// The rail counts what is a key away, and says what the plan implies past its tasks,
/// counted once across its shapes.
#[test]
fn the_rail_counts_what_was_decided_and_what_the_plan_implies() {
    let review = global_packs();
    let reading = Reading::of(&review, Vec::new());
    let groups = rail(&reading, 1);
    let tallies: Vec<(&str, usize, Option<char>)> = groups
        .iter()
        .flat_map(|group| &group.rows)
        .filter_map(|row| match row {
            RailRow::Tally { what, count, key } => Some((what.as_str(), *count, *key)),
            _ => None,
        })
        .collect();
    assert_eq!(
        tallies,
        [
            ("decided for you", 4, Some('d')),
            ("risks", review.plan.risks.len(), Some('d')),
            ("left out", 2, Some('d')),
        ]
    );
    let implies = group(&groups, "implies").expect("what it implies");
    assert!(
        matches!(&implies.rows[0], RailRow::Weighs(said) if said == "what it changes is named in 5 files outside every task"),
        "{}",
        implies.rows.len()
    );
}

#[test]
fn a_shape_named_in_too_many_files_is_said_by_how_many() {
    let mut review = global_packs();
    let read = review.shapes_read.as_mut().expect("the fixture's reading");
    set_common(read, 214);
    let reading = Reading::of(&review, Vec::new());
    let groups = rail(&reading, 1);
    let implies = group(&groups, "implies").expect("what it implies");
    assert!(
        implies
            .rows
            .iter()
            .any(|row| matches!(row, RailRow::Weighs(said) if said == "`resolve_workflow` is named in 214 files, too many to follow")),
    );
}

/// The reading of `resolve_workflow` as a name too many files carry.
fn set_common(read: &mut ShapesReadPayload, files: usize) {
    for shape in &mut read.shapes {
        if shape.name == "resolve_workflow" {
            if let InTree::There { outside, .. } = &mut shape.in_tree {
                *outside = Outside::Common { files };
            }
        }
    }
}

/// A plan that keeps itself from being proven says so before anything else it counts.
#[test]
fn what_keeps_a_plan_from_being_proven_stands_in_the_rail() {
    let review = greeting();
    assert!(
        !review.flaws().is_empty(),
        "this plan cannot be proven as written"
    );
    let reading = Reading::of(&review, Vec::new());
    let groups = rail(&reading, 1);
    let weighing: Vec<&str> = groups
        .iter()
        .flat_map(|group| &group.rows)
        .filter_map(|row| match row {
            RailRow::Weighs(said) => Some(said.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        weighing
            .iter()
            .any(|said| said.contains("from being proven as it is written")),
        "{}",
        weighing.join("\n")
    );
}

/// What the plan says of itself is its description and its design, with its drawings.
#[test]
fn the_change_is_what_the_author_wrote() {
    let review = greeting();
    let reading = Reading::of(&review, Vec::new());
    let spot = Spot {
        at: 0,
        view: View::Stop,
        top: 0,
        choice: 0,
    };
    let rows = read(&reading, &spot);
    assert!(
        holds(&rows, "The program says nothing today."),
        "{}",
        rows.join("\n")
    );
    assert!(holds(&rows, "owns the words"), "{}", rows.join("\n"));
    assert!(
        !holds(&rows, "What does it say"),
        "what it decided is a key away: {}",
        rows.join("\n")
    );
}

/// A card has the change; its tests are a key away.
#[test]
fn a_card_has_no_tests_and_its_proof_has_them_whole() {
    let review = global_packs();
    let reading = Reading::of(&review, Vec::new());
    let mut spot = Spot {
        at: 1,
        view: View::Stop,
        top: 0,
        choice: 0,
    };
    let card = read(&reading, &spot);
    assert!(holds(&card, "you will see"), "{}", card.join("\n"));
    assert!(holds(&card, "PackInstallScope"), "{}", card.join("\n"));
    assert!(
        !holds(&card, "global_pack_storage.rs"),
        "{}",
        card.join("\n")
    );

    spot.view = View::Proof;
    let proof = read(&reading, &spot);
    assert!(holds(&proof, "tested by"), "{}", proof.join("\n"));
    assert!(holds(&proof, "the suite"), "{}", proof.join("\n"));
    assert!(holds(&proof, "cargo clippy"), "{}", proof.join("\n"));
    assert!(
        holds(&proof, "global_pack_storage.rs"),
        "the file the spec wrote: {}",
        proof.join("\n")
    );
    assert!(
        holds(
            &proof,
            "pack::installed_packs(pack::PackInstallScope::Project"
        ),
        "whole: {}",
        proof.join("\n")
    );
    assert!(
        !holds(&proof, "PackInstallScope {"),
        "and not the change: {}",
        proof.join("\n")
    );
}

/// One decision is open at a time, under a pointer. What a person answered, the risks and
/// what is left out stand under them whole.
#[test]
fn what_was_decided_shows_one_decision_open() {
    let review = greeting();
    let reading = Reading::of(&review, Vec::new());
    assert_eq!(reading.choices(), 1, "one was made for the person");
    let spot = Spot {
        at: 0,
        view: View::Choices,
        top: 0,
        choice: 0,
    };
    let rows = read(&reading, &spot);
    assert!(
        holds(&rows, "▸ 1 What does it say when no name is given?"),
        "{}",
        rows.join("\n")
    );
    assert!(holds(&rows, "It greets"), "{}", rows.join("\n"));
    assert!(holds(&rows, "Refuse to run"), "{}", rows.join("\n"));
    assert!(holds(&rows, "you answered"), "{}", rows.join("\n"));
    assert!(holds(&rows, "risks"), "{}", rows.join("\n"));
    assert!(holds(&rows, "breaks the line"), "{}", rows.join("\n"));
    assert!(holds(&rows, "left out"), "{}", rows.join("\n"));

    let packs = global_packs();
    let reading = Reading::of(&packs, Vec::new());
    assert_eq!(reading.choices(), 4);
    let spot = Spot { choice: 2, ..spot };
    let rows = read(&reading, &spot);
    assert!(holds(&rows, "▸ 3 What happens when"), "{}", rows.join("\n"));
    assert!(
        holds(&rows, "  1 What scope and persistence"),
        "{}",
        rows.join("\n")
    );
    assert_eq!(
        rows.iter().filter(|row| row.contains("chose")).count(),
        1,
        "{}",
        rows.join("\n")
    );
}

/// What a decision's card says of the plan: what it is about, unless the question says
/// it, and how big it is.
#[test]
fn a_card_says_what_the_plan_is_about_and_how_big_it_is() {
    let review = global_packs();
    let said = about(
        &review,
        "Plan and its tests registered. Approve?",
        Glyphs::Ascii,
    );
    assert_eq!(
        said,
        [
            "Install packs in the user's Yunta state directory for use across projects",
            "4 tasks in 3 steps ; 4 decided for you ; 2 risks ; 2 left out",
            "what it changes is named in 5 files outside every task",
        ]
    );
    let said = about(
        &review,
        "Approve the plan: Install packs in the user's Yunta state directory for use across projects.?",
        Glyphs::Ascii,
    );
    assert_eq!(
        said.len(),
        2,
        "the question already said it: {}",
        said.join("\n")
    );
}

/// The whole desk over the plan these tests read, at the size a person reads it at, for
/// each thing a person can have in front of them.
#[test]
fn the_desk_matches_its_goldens() {
    let review: PlanReview = global_packs();
    let reading = Reading::of(&review, Vec::new());
    let foot = |keys: [&str; 4]| DeskFoot {
        question: "Plan and its tests registered. Approve?".to_string(),
        row: FootRow::Options(vec![
            "approve".to_string(),
            "adjust".to_string(),
            "abort".to_string(),
        ]),
        keys: keys.map(str::to_string).to_vec(),
    };
    let reading_keys = ["j k task", "p proof", "d decided for you", "esc back"];
    for (name, at, view, choice, keys) in [
        ("desk-the-change", 0, View::Stop, 0, reading_keys),
        ("desk-task", 2, View::Stop, 0, reading_keys),
        (
            "desk-proof",
            1,
            View::Proof,
            0,
            ["j k task", "p the card", "d decided for you", "esc back"],
        ),
        (
            "desk-choices",
            1,
            View::Choices,
            1,
            ["j k choice", "p proof", "d the tasks", "esc back"],
        ),
    ] {
        let look = look(118);
        let spot = Spot {
            at,
            view,
            top: 0,
            choice,
        };
        let laid = reading
            .desk(
                &spot,
                &["plan", "EKPBGE", "fragua (standard)"].map(str::to_string),
                Some("needs you  approve-plan".to_string()),
                foot(keys),
                &look,
            )
            .drawn(&look, 36);
        let text: String = laid
            .placed
            .lines
            .iter()
            .map(|line| format!("{}\n", line.text().trim_end()))
            .collect();
        assert_golden(&goldens().join(format!("{name}.txt")), &text);
    }
}
