//! A plan's rail at its desk: the stops a person walks, in the order they walk
//! them, and what the rail says of the whole beside them.

use surco::components::{RailGroup, RailRow};
use surco::glyphs::Glyphs;
use yunta_core::events::{InTree, Outside, ShapesReadPayload};

use super::super::deciding::brief;
use super::super::documents::{global_packs, review as greeting, shown_plan};
use super::{designed, holds, rail, read, stops};
use crate::page::desk::{about, reading, Reading, Spot, View};

/// Each tally of `groups`: what it counts, its count, and the key that opens it.
fn tallies(groups: &[RailGroup]) -> Vec<(&str, usize, Option<char>)> {
    groups
        .iter()
        .flat_map(|group| &group.rows)
        .filter_map(|row| match row {
            RailRow::Tally { what, count, key } => Some((what.as_str(), *count, *key)),
            _ => None,
        })
        .collect()
}

/// The design's plan on its rail: its places in one untitled group, the change and its
/// one task with its step's number; then `what it says`, a count a row, `d` at the far end
/// of what was decided for the person, the key that opens it. Nothing more: what the plan
/// implies, its order and what each task changes are its printed page's.
#[test]
fn the_rail_holds_its_places_then_what_it_says() {
    let reading = Reading::of(designed());
    let groups = rail(&reading, 0);
    let titles: Vec<Option<&str>> = groups.iter().map(|group| group.title.as_deref()).collect();
    assert_eq!(titles, [None, Some("what it says")]);
    assert_eq!(
        stops(&groups),
        [
            (String::new(), "the change".to_string(), true),
            ("1".to_string(), "global-pack-install".to_string(), false),
        ]
    );
    assert_eq!(
        tallies(&groups),
        [
            ("decided for you", 3, Some('d')),
            ("out of scope", 3, None),
            ("risks", 2, None),
        ]
    );
}

/// A plan that keeps itself from being proven says so last on its rail, under
/// `what weighs on it`, each sentence a row of its own.
#[test]
fn what_keeps_a_plan_from_being_proven_weighs_on_it_last() {
    let review = greeting();
    assert!(
        !review.flaws().is_empty(),
        "this plan cannot be proven as written"
    );
    let groups = rail(&Reading::of(review), 1);
    let last = groups.last().expect("a rail");
    assert_eq!(last.title.as_deref(), Some("what weighs on it"));
    let weighing: Vec<&str> = last
        .rows
        .iter()
        .filter_map(|row| match row {
            RailRow::Weighs(said) => Some(said.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(weighing.len(), last.rows.len(), "only what weighs");
    assert!(
        weighing
            .iter()
            .any(|said| said.contains("from being proven as it is written")),
        "{}",
        weighing.join("\n")
    );
}

/// A person starts on what the plan says of itself, and then walks its tasks, step by
/// step: a step's number stands on its first task.
#[test]
fn the_stops_run_from_what_the_plan_says_through_each_task_in_its_step() {
    let review = global_packs();
    let reading = Reading::of(review.clone());
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
    let reading = Reading::of(review.clone());
    assert_eq!(reading.stops(), 2);
    assert_eq!(stops(&rail(&reading, 0))[0].1, "greet");
    assert!(stops(&rail(&reading, 0))[0].2);
}

/// What else a decision shows beside its plan, before it or after it, is no place of the
/// desk's: its places are the plan's own, what it says of itself and each task, and no place
/// reads the other document.
#[test]
fn what_else_a_decision_shows_is_no_place_at_its_desk() {
    let review = greeting();
    let alone = Reading::of(review.clone());
    for documents in [
        [brief("what was asked"), shown_plan(review.clone())],
        [shown_plan(review.clone()), brief("what was asked")],
    ] {
        let reading = reading(&documents).expect("a plan among them");
        assert_eq!(reading.stops(), alone.stops());
        let names: Vec<String> = stops(&rail(&reading, 0))
            .into_iter()
            .map(|(_, name, _)| name)
            .collect();
        assert_eq!(names[0], "the change");
        assert_eq!(names[1], "greet");
        for at in 0..reading.stops() {
            let spot = Spot {
                at,
                view: View::Stop,
            };
            assert!(!holds(&read(&reading, &spot), "what was asked"));
        }
    }
}

/// The rail counts what a key away says of the plan: what was decided for the person,
/// what it leaves out and its risks, `d` on the first, the key that opens them.
#[test]
fn the_rail_counts_what_was_decided_what_is_left_out_and_the_risks() {
    let review = global_packs();
    let reading = Reading::of(review.clone());
    assert_eq!(
        tallies(&rail(&reading, 1)),
        [
            ("decided for you", 4, Some('d')),
            ("out of scope", 2, None),
            ("risks", review.plan.risks.len(), None),
        ]
    );
}

/// What a shape implies past the plan's tasks is said beside the card, by how many files
/// name it where they are too many to follow.
#[test]
fn a_shape_named_in_too_many_files_is_said_by_how_many() {
    let mut review = global_packs();
    let read = review.shapes_read.as_mut().expect("the fixture's reading");
    set_common(read, 214);
    let said = about(&review, "Approve?", Glyphs::Ascii);
    assert!(
        said.iter()
            .any(|line| line == "`resolve_workflow` is named in 214 files, too many to follow"),
        "{}",
        said.join("\n")
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
