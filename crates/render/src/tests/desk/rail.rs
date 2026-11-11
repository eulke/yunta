//! A plan's rail at its desk: the stops a person walks, in the order they walk
//! them, and what the rail says of the whole beside them.

use surco::components::RailRow;
use yunta_core::events::{InTree, Outside, ShapesReadPayload};

use super::super::deciding::brief;
use super::super::documents::{global_packs, review as greeting, run};
use super::{group, holds, rail, read, stops};
use crate::page::desk::{AlsoShown, Reading, Spot, View};
use crate::page::Form;

/// A person starts on what the plan says of itself, and then walks its tasks, step by
/// step: a step's number stands on its first task.
#[test]
fn the_stops_run_from_what_the_plan_says_through_each_task_in_its_step() {
    let review = global_packs();
    let reading = Reading::of(review.clone(), Vec::new());
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
    let reading = Reading::of(review.clone(), Vec::new());
    assert_eq!(reading.stops(), 2);
    assert_eq!(stops(&rail(&reading, 0))[0].1, "greet");
    assert!(stops(&rail(&reading, 0))[0].2);
}

/// What else the decision shows is one more stop, in its own group.
#[test]
fn what_else_a_decision_shows_is_a_stop_too() {
    let review = greeting();
    let brief = brief("what was asked");
    let run = run();
    let also = vec![AlsoShown {
        name: "the brief".to_string(),
        document: brief.clone(),
        run: run.clone(),
        form: Form::Review,
        leads: false,
    }];
    let reading = Reading::of(review.clone(), also);
    assert_eq!(reading.stops(), 4);
    let groups = rail(&reading, 3);
    let also = group(&groups, "also shown").expect("its group");
    assert!(
        matches!(&also.rows[0], RailRow::Stop { name, pointed: true, .. } if name == "the brief"),
    );
    let spot = Spot {
        at: 3,
        view: View::Stop,
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
    let brief = brief("what was asked");
    let run = run();
    let also = vec![AlsoShown {
        name: "the brief".to_string(),
        document: brief.clone(),
        run: run.clone(),
        form: Form::Review,
        leads: true,
    }];
    let reading = Reading::of(review.clone(), also);
    let shown = stops(&rail(&reading, 0));
    assert_eq!(shown[0], (String::new(), "the brief".to_string(), true));
    assert_eq!(shown[1].1, "the change");
    assert_eq!(shown[2].1, "greet");
    let spot = Spot {
        at: 0,
        view: View::Stop,
        choice: 0,
    };
    assert!(holds(&read(&reading, &spot), "what was asked"));
}

/// The order is what each task waits on, named without what every name repeats.
#[test]
fn the_rail_says_the_order_and_who_changes_what_by_short_names() {
    let review = global_packs();
    let reading = Reading::of(review.clone(), Vec::new());
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
    let reading = Reading::of(review.clone(), Vec::new());
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
    let reading = Reading::of(review.clone(), Vec::new());
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
    let reading = Reading::of(review.clone(), Vec::new());
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
    let reading = Reading::of(review.clone(), Vec::new());
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
