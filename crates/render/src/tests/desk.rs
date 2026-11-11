//! A plan as its desk reads it: every place a person can stand on, what each shows, and
//! what the rail says of the whole, from the plan the rest of these tests read.

use surco::components::{Desk, DeskFoot, FootRow, Legend, RailGroup, RailRow};
use surco::glyphs::Glyphs;
use surco::golden::assert_golden;
use surco::line::Line;
use surco::look::{Look, Width};
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_core::shown::PlanReview;

use super::documents::{global_packs, goldens, review as greeting};
use crate::page::desk::{about, Reading, Spot, View};

mod rail;

fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: surco::motion::Motion::Moves,
    }
}

/// What a person spot on `spot` reads in the pane of a terminal 80 cells wide, laid out at
/// the pane's measure as the desk lays it out.
fn read(reading: &Reading, spot: &Spot) -> Vec<String> {
    let terminal = look(80);
    let measure = Look {
        width: Width::row(Desk::pane_cells(&terminal), None),
        ..terminal
    };
    Terminal::on(measure)
        .lines(&reading.pane_doc(spot, &terminal))
        .iter()
        .map(Line::text)
        .collect()
}

fn holds(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|row| row.contains(needle))
}

/// The rail as a person spot on `at` reads it.
fn rail(reading: &Reading, at: usize) -> Vec<RailGroup> {
    reading.rail(&Spot {
        at,
        view: View::Stop,
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

/// What the plan says of itself is its description and its design, with its drawings.
#[test]
fn the_change_is_what_the_author_wrote() {
    let review = greeting();
    let reading = Reading::of(review.clone(), Vec::new());
    let spot = Spot {
        at: 0,
        view: View::Stop,
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
    let reading = Reading::of(review.clone(), Vec::new());
    let mut spot = Spot {
        at: 1,
        view: View::Stop,
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
    let reading = Reading::of(review.clone(), Vec::new());
    assert_eq!(reading.choices(), 1, "one was made for the person");
    let spot = Spot {
        at: 0,
        view: View::Choices,
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
    let reading = Reading::of(packs.clone(), Vec::new());
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
    let reading = Reading::of(review.clone(), Vec::new());
    let foot = |[moves, p, d]: [&str; 3]| DeskFoot {
        question: "Plan and its tests registered. Approve?".to_string(),
        row: FootRow::Options {
            named: vec![
                "approve".to_string(),
                "adjust".to_string(),
                "abort".to_string(),
            ],
            more: 0,
        },
        legend: Legend::new()
            .key("j k", moves)
            .key("p", p)
            .key("d", d)
            .key("esc", "back"),
    };
    let reading_keys = ["task", "proof", "decided for you"];
    for (name, at, view, choice, keys) in [
        ("desk-the-change", 0, View::Stop, 0, reading_keys),
        ("desk-task", 2, View::Stop, 0, reading_keys),
        (
            "desk-proof",
            1,
            View::Proof,
            0,
            ["task", "the card", "decided for you"],
        ),
        (
            "desk-choices",
            1,
            View::Choices,
            1,
            ["choice", "proof", "the tasks"],
        ),
    ] {
        let look = look(118);
        let spot = Spot { at, view, choice };
        let laid = reading
            .desk(
                &spot,
                0,
                &["plan", "EKPBGE", "fragua (standard)"].map(str::to_string),
                Some("needs you  approve-plan".to_string()),
                foot(keys),
                &look,
            )
            .drawn(&look, 36);
        let text: String = laid
            .lines
            .iter()
            .map(|line| format!("{}\n", line.text().trim_end()))
            .collect();
        assert_golden(&goldens().join(format!("{name}.txt")), &text);
    }
}
