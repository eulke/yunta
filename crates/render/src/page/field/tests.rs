use std::time::Duration;

use surco::doc::Depth;
use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use surco::width::cell_width;
use surco::words::Standing;
use yunta_core::events::PauseReason;
use yunta_core::{ModeName, NodeId, Pid, RunId};
use yunta_replay::frame::{RunPhase, WaitingOn};
use yunta_replay::registry::{Engine, Prompt};
use yunta_testkit_core::fixed_now;

use super::*;
use crate::page::run::fixtures::{ahead, finished, frame, parked_on_lint};

/// A run of the `review` workflow, `id`, in `group`, where it has stood for
/// `age_secs`: its first node finished, its second standing as the run is
/// called, its third never run. A run that needs a person waits on a
/// failed `lint`; one a person stopped was interrupted; one still moving
/// is running; one closed finished.
fn row(id: &'static str, group: FieldGroup, age_secs: u64) -> FieldRow {
    let (word, phase) = match group {
        FieldGroup::NeedsYou => (Standing::NeedsYou, parked_on_lint()),
        FieldGroup::Stalled => (Standing::Stalled, RunPhase::Running),
        FieldGroup::Stopped => (
            Standing::Stopped,
            RunPhase::Waiting {
                on: WaitingOn::Run {
                    reason: PauseReason::Cancelled,
                },
            },
        ),
        FieldGroup::Running => (Standing::Running, RunPhase::Running),
        FieldGroup::Closed => (Standing::Finished, RunPhase::Finished),
    };
    let mut work = finished("work");
    work.standing = word;
    let mut ship = ahead("ship");
    ship.standing = Standing::NeverRan;
    let mut frame = frame(phase, vec![finished("plan"), work, ship], 0);
    frame.run_id = RunId::from_static(id);
    frame.workflow = "review".into();
    frame.mode = ModeName::default();
    frame.standing = word;
    frame.since_last_event = Some(Duration::from_secs(age_secs));
    FieldRow { frame, menu: false }
}

/// The field of `rows` and `unreadable`, as deep as `depth`, drawn unpainted.
fn render_at(rows: Vec<FieldRow>, unreadable: Vec<UnreadableRun>, depth: Depth) -> String {
    let look = Look::plain();
    Terminal::on(look).draw(&field_page(&rows, &unreadable, depth).document(&look))
}

/// The field of this repository's `rows` and `unreadable`, as deep as `depth`.
fn field_page<'a>(
    rows: &'a [FieldRow],
    unreadable: &'a [UnreadableRun],
    depth: Depth,
) -> Field<'a> {
    Field {
        rows,
        unreadable,
        elsewhere: 0,
        here: true,
        storage: "/home/me/.yunta",
        depth,
    }
}

fn render(rows: Vec<FieldRow>) -> String {
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
        row(NEWEST, FieldGroup::NeedsYou, 30),
        row(OLDEST, FieldGroup::NeedsYou, 86_400),
    ]);
    assert_eq!(listed(&text), handles(&[OLDEST, NEWEST]), "{text}");
}

#[test]
fn runs_that_arrived_together_are_ordered_by_id() {
    let text = render(vec![
        row(TIED_B, FieldGroup::Running, 60),
        row(TIED_A, FieldGroup::Running, 60),
    ]);
    assert_eq!(listed(&text), handles(&[TIED_A, TIED_B]), "{text}");
}

#[test]
fn what_needs_a_person_is_listed_above_what_is_only_moving() {
    let text = render_at(
        vec![
            row(NEWEST, FieldGroup::Closed, 900),
            row(TIED_A, FieldGroup::Running, 600),
            row(OLDEST, FieldGroup::NeedsYou, 5),
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
    let closed: Vec<FieldRow> = (0..12u64)
        .map(|at| {
            let mut closed = row(OLDEST, FieldGroup::Closed, 60 * (at + 1));
            closed.frame.run_id = RunId::from(format!("01JBZ5X8K3N7Q2W6E4R9T1Y0P{at:X}").as_str());
            closed
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
            UnreadableRun::new(&RunId::from_static(NEWEST), "gone".to_string()),
            UnreadableRun::new(&RunId::from_static(TIED_A), "gone".to_string()),
        ]
    };
    let glance = render_at(
        vec![row(OLDEST, FieldGroup::Running, 10)],
        unreadable(),
        Depth::Glance,
    );
    assert!(
        glance.contains("T1Y0P2, T1Y0P4 could not be read"),
        "{glance}"
    );
    let full = render_at(
        vec![row(OLDEST, FieldGroup::Running, 10)],
        unreadable(),
        Depth::Full,
    );
    assert_eq!(listed(&full), handles(&[OLDEST, TIED_A, NEWEST]), "{full}");
}

#[test]
fn a_stopped_run_ends_on_the_command_that_continues_it() {
    let text = render(vec![row(OLDEST, FieldGroup::Stopped, 5)]);
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
        row(OLDEST, FieldGroup::Running, 5 * 86_400 + 13 * 3_600),
        row(NEWEST, FieldGroup::Running, 30),
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

/// A field with a run in every group a reader acts on.
fn field() -> Vec<FieldRow> {
    // Asked at its terminal, by an engine alive now, about `implement`.
    let mut asking = row(OLDEST, FieldGroup::NeedsYou, 32 * 60);
    asking.frame.phase = RunPhase::Running;
    let since = fixed_now() - chrono::Duration::minutes(32);
    let engine = Engine::Alive {
        pid: Pid::current(),
        prompt: Some(Prompt {
            node: Some(NodeId::from("implement")),
            since,
        }),
        words: None,
    };
    asking.frame.asking = engine.asking(fixed_now());
    vec![
        asking,
        row(TIED_A, FieldGroup::NeedsYou, 22 * 86_400),
        row(TIED_B, FieldGroup::Stopped, 18 * 86_400),
        row(NEWEST, FieldGroup::Closed, 8 * 3_600),
    ]
}

#[test]
fn the_field_matches_its_goldens() {
    let goldens = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/field");
    for environment in &ENVIRONMENTS {
        let look = Look::of(environment);
        let rows = field();
        let page = field_page(&rows, &[], Depth::Glance).document(&look);
        assert_golden(
            &environment.golden(&goldens, "runs"),
            &Terminal::on(look).draw(&page),
        );
    }
}

#[test]
fn the_word_a_run_is_called_by_decides_the_group_it_is_listed_under() {
    // A log that stopped making sense needs a person as much as a
    // decision does: nothing moves it on its own again.
    for word in [Standing::NeedsYou, Standing::Broken] {
        assert_eq!(FieldGroup::of(word, false), FieldGroup::NeedsYou, "{word}");
    }
    for word in [Standing::Created, Standing::Running, Standing::Waiting] {
        assert_eq!(FieldGroup::of(word, false), FieldGroup::Running, "{word}");
    }
    assert_eq!(
        FieldGroup::of(Standing::Stopped, false),
        FieldGroup::Stopped
    );
    for word in [
        Standing::Finished,
        Standing::Reported,
        Standing::Failed,
        Standing::Stopped,
        Standing::Promoted,
    ] {
        assert_eq!(FieldGroup::of(word, true), FieldGroup::Closed, "{word}");
    }
}

#[test]
fn every_word_a_run_can_be_called_by_has_a_group() {
    // The listing is an inbox: a run whose word fell through would
    // be a run nobody goes looking for.
    let groups: Vec<FieldGroup> = Standing::ALL
        .into_iter()
        .map(|word| FieldGroup::of(word, false))
        .collect();
    assert_eq!(groups.len(), Standing::ALL.len());
}
