use std::time::Duration;

use surco::doc::Depth;
use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::width::cell_width;
use surco::words::Standing;
use yunta_core::{ModeName, RunId};

use super::*;
use crate::commands::list::inbox::Unreadable;

fn row(id: &'static str, group: Group, age_secs: u64) -> RunRow {
    let word = match group {
        Group::NeedsYou => Standing::NeedsYou,
        Group::Stalled => Standing::Stalled,
        Group::Stopped => Standing::Stopped,
        Group::InFlight => Standing::Running,
        Group::Closed => Standing::Finished,
    };
    RunRow {
        run_id: RunId::from_static(id),
        group,
        word,
        workflow: "review".into(),
        mode: ModeName::default(),
        age: Duration::from_secs(age_secs),
        reason: None,
        command: None,
        said: String::new(),
        furrow: vec![Standing::Finished, word, Standing::NeverRan],
        asking: false,
    }
}

fn answer(rows: Vec<RunRow>, unreadable: Vec<Unreadable>, depth: Depth) -> ListRunsAnswer {
    ListRunsAnswer {
        schema_version: SCHEMA_VERSION,
        storage: "/home/me/.yunta".to_string(),
        inbox: Inbox {
            rows,
            unreadable,
            elsewhere: 0,
            here: true,
        },
        depth,
    }
}

/// The field drawn the way the command draws it, unpainted.
fn render_at(rows: Vec<RunRow>, unreadable: Vec<Unreadable>, depth: Depth) -> String {
    let look = Look::plain();
    crate::render::draw(answer(rows, unreadable, depth).document(&look), &look)
}

fn render(rows: Vec<RunRow>) -> String {
    render_at(rows, Vec::new(), Depth::Glance)
}

/// The handles a field names, in the order it names them: each row's,
/// and each unreadable run's `handle: problem`.
fn listed(text: &str) -> Vec<&str> {
    text.lines()
        // What to type next names a run too, and is not a row.
        .filter(|line| !line.trim_start().starts_with("yunta "))
        .filter_map(|line| {
            line.split_whitespace()
                .map(|word| word.trim_end_matches(':'))
                .find(|word| {
                    word.len() == RunId::HANDLE_CHARS
                        && word
                            .chars()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                })
        })
        .collect()
}

/// What each of `ids` is called by on a line a person reads.
fn handles(ids: &[&'static str]) -> Vec<&'static str> {
    ids.iter()
        .map(|id| &id[id.len() - RunId::HANDLE_CHARS..])
        .collect()
}

const OLDEST: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P1";
const TIED_A: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P2";
const TIED_B: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P3";
const NEWEST: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P4";

#[test]
fn a_group_answers_the_run_that_has_waited_longest_first() {
    let text = render(vec![
        row(NEWEST, Group::NeedsYou, 30),
        row(OLDEST, Group::NeedsYou, 86_400),
    ]);
    assert_eq!(listed(&text), handles(&[OLDEST, NEWEST]), "{text}");
}

#[test]
fn runs_that_arrived_together_are_ordered_by_id() {
    let text = render(vec![
        row(TIED_B, Group::InFlight, 60),
        row(TIED_A, Group::InFlight, 60),
    ]);
    assert_eq!(listed(&text), handles(&[TIED_A, TIED_B]), "{text}");
}

#[test]
fn what_needs_a_person_is_listed_above_what_is_only_moving() {
    let text = render_at(
        vec![
            row(NEWEST, Group::Closed, 900),
            row(TIED_A, Group::InFlight, 600),
            row(OLDEST, Group::NeedsYou, 5),
        ],
        Vec::new(),
        Depth::Full,
    );
    assert_eq!(listed(&text), handles(&[OLDEST, TIED_A, NEWEST]), "{text}");
    let first = text.lines().next().unwrap_or_default();
    assert!(
        first.contains("1 needs you ; 1 running ; 1 closed"),
        "the headline counts every group: {text}"
    );
}

#[test]
fn a_glance_counts_the_closed_runs_and_the_whole_story_lists_them_newest_first() {
    let closed: Vec<RunRow> = (0..12u64)
        .map(|at| RunRow {
            run_id: RunId::from(format!("01JBZ5X8K3N7Q2W6E4R9T1Y0P{at:X}").as_str()),
            ..row(OLDEST, Group::Closed, 60 * (at + 1))
        })
        .collect();
    let glance = render(closed.clone());
    assert!(listed(&glance).is_empty(), "{glance}");
    // The headline counts the closed runs; the line says how recent the newest is.
    assert!(glance.contains("the newest closed 1m ago"), "{glance}");
    assert!(glance.contains("12 closed"), "{glance}");
    let full = render_at(closed, Vec::new(), Depth::Full);
    assert_eq!(listed(&full).len(), 12, "{full}");
    assert_eq!(listed(&full).first(), Some(&"T1Y0P0"), "{full}");
}

#[test]
fn a_run_that_does_not_read_back_is_counted_and_listed_in_the_whole_story_by_id() {
    let unreadable = || {
        vec![
            Unreadable::new(&RunId::from_static(NEWEST), "gone".to_string()),
            Unreadable::new(&RunId::from_static(TIED_A), "gone".to_string()),
        ]
    };
    let glance = render_at(
        vec![row(OLDEST, Group::InFlight, 10)],
        unreadable(),
        Depth::Glance,
    );
    assert!(
        glance.contains("T1Y0P2, T1Y0P4 could not be read"),
        "{glance}"
    );
    let full = render_at(
        vec![row(OLDEST, Group::InFlight, 10)],
        unreadable(),
        Depth::Full,
    );
    assert_eq!(listed(&full), handles(&[OLDEST, TIED_A, NEWEST]), "{full}");
}

#[test]
fn a_stopped_run_ends_on_the_command_that_continues_it() {
    let stopped = RunRow {
        said: interrupted(),
        command: Some(format!("yunta resume {}", handles(&[OLDEST])[0])),
        ..row(OLDEST, Group::Stopped, 5)
    };
    let text = render(vec![stopped]);
    assert!(
        text.lines().any(|line| line
            .trim_end()
            .ends_with(&format!("yunta resume {}", handles(&[OLDEST])[0]))),
        "{text}"
    );
}

#[test]
fn every_row_fits_the_line_and_its_age_ends_in_one_column() {
    let text = render(vec![
        row(OLDEST, Group::InFlight, 5 * 86_400 + 13 * 3_600),
        row(NEWEST, Group::InFlight, 30),
    ]);
    let rows: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("review"))
        .collect();
    assert_eq!(rows.len(), 2, "{text}");
    assert!(rows[0].ends_with("5d"), "{text}");
    assert_eq!(cell_width(rows[0]), cell_width(rows[1]), "{text}");
    for line in text.lines() {
        assert!(cell_width(line) <= Look::plain().width.cells(), "{line}");
    }
}

/// What the row of a run a person interrupted says.
fn interrupted() -> String {
    yunta_render::pause::sentence(&yunta_core::events::PauseReason::Cancelled)
}

/// A field with a run in every group a reader acts on.
fn field() -> Vec<RunRow> {
    let asking = RunRow {
        said: "node `implement` asks at its terminal".to_string(),
        asking: true,
        ..row(OLDEST, Group::NeedsYou, 32 * 60)
    };
    let parked = RunRow {
        said: "node `lint` failed and its 0 re-routes to `fix-lint` are exhausted".to_string(),
        ..row(TIED_A, Group::NeedsYou, 22 * 86_400)
    };
    let stopped = RunRow {
        said: interrupted(),
        command: Some("yunta resume T1Y0P3".to_string()),
        ..row(TIED_B, Group::Stopped, 18 * 86_400)
    };
    vec![
        asking,
        parked,
        stopped,
        row(NEWEST, Group::Closed, 8 * 3_600),
    ]
}

#[test]
fn the_field_matches_its_goldens() {
    let goldens = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/field");
    for environment in &ENVIRONMENTS {
        let look = Look::of(environment);
        let page = answer(field(), Vec::new(), Depth::Glance).document(&look);
        assert_golden(
            &environment.golden(&goldens, "runs"),
            &crate::render::draw(page, &look),
        );
    }
}
