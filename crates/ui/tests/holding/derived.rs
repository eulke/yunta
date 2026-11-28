//! See [`super`]. A run's frame is derived once for each head of its log, by the run the
//! state keeps: read whole, it is derived once; drawn twice with no news between, it is
//! derived no more; a piece of news derives it once more; and drawn a second later with no
//! news it is derived no more, while every time on the screen that moves with the clock
//! reads a second on. The count is the kept run's own, read off the state, never a timer.

use chrono::{DateTime, Utc};
use surco::line::Line;
use surco::surface::Terminal;
use yunta_core::events::{EventPayload, RunEvent};
use yunta_testkit_ui::screens::run;
use yunta_ui::{Input, State};

use super::{head, logged, moving, WIDEST};

/// How long after the run's last event the screen is first drawn: a few seconds, so its
/// times stand apart from its last event's.
const SECONDS_AFTER_ITS_LAST_EVENT: i64 = 5;

/// The rows a screen's run stands in across it at its tallest: its headline, its ribbon's
/// names and strokes, its gist, and a blank row.
const ACROSS: usize = 6;

/// The rows a run's pane is drawn in: the design's widest screen's.
const PANE: usize = 26;

/// How many times the frame of the run `state` keeps in view was derived, as that run
/// counts them.
fn derived(state: &State) -> u64 {
    let in_view = state.followed_runs().in_view.as_deref();
    in_view.expect("the run in view").run.derivations()
}

/// `state`'s screen drawn whole, 120 columns wide: the trail, what stands across, the pane
/// under its title, the meter, the attention row and the legend, each row's text.
fn drawn(state: &State) -> Vec<String> {
    let screen = state.screen(&moving(WIDEST));
    let terminal = Terminal::on(moving(WIDEST));
    let mut lines = vec![terminal.trail(&screen.trail, &screen.tally)];
    lines.extend(
        screen
            .about
            .iter()
            .flat_map(|about| terminal.across(about, ACROSS)),
    );
    lines.push(terminal.title(&screen.main, false));
    lines.extend(terminal.pane(&screen.main, PANE).lines);
    lines.extend(screen.meter.iter().map(|meter| terminal.meter(meter)));
    lines.push(terminal.legend(&screen.legend, &screen.anywhere));
    lines.iter().map(Line::text).collect()
}

/// The seconds `word` says where it is a time as a screen says one, a figure and its unit
/// once or twice (`9s`, `7m34s`, `2h05m`); none for any other word.
fn seconds(word: &str) -> Option<i64> {
    let mut total = 0;
    let mut rest = word;
    let mut parts = 0;
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !c.is_ascii_digit())?;
        let (figure, after) = rest.split_at(digits);
        let mut after = after.chars();
        let unit = match after.next()? {
            'd' => 86_400,
            'h' => 3_600,
            'm' => 60,
            's' => 1,
            _ => return None,
        };
        total += figure.parse::<i64>().ok()? * unit;
        rest = after.as_str();
        parts += 1;
    }
    (1..=2).contains(&parts).then_some(total)
}

/// Each word of `rows` that says a time, in seconds, in the order the rows say them.
fn times(rows: &[String]) -> Vec<i64> {
    let words = rows
        .iter()
        .flat_map(|row| row.split(|c: char| !c.is_ascii_alphanumeric()));
    words.filter_map(seconds).collect()
}

/// How long the run of `events` has gone at `now`, in seconds, from its creation.
fn gone(events: &[yunta_core::events::StoredEvent], now: DateTime<Utc>) -> i64 {
    let created = events.iter().find_map(|event| match event.payload()? {
        EventPayload::Run(RunEvent::Created(_)) => Some(event.timestamp),
        _ => None,
    });
    let created = created.expect("the run's log opens with its creation");
    (now - created).num_seconds()
}

#[tokio::test]
async fn a_runs_frame_is_derived_once_read_whole_and_once_more_for_each_piece_of_news() {
    let mut screen = run::opened("work").await;
    assert_eq!(
        derived(screen.state()),
        1,
        "the run read whole, its frame derived once"
    );
    screen.state().screen(&moving(WIDEST));
    screen.state().screen(&moving(WIDEST));
    assert_eq!(
        derived(screen.state()),
        1,
        "two drawings with no news between them derive nothing"
    );
    screen.followed_on_to(head(&screen) + 1).await;
    assert_eq!(
        derived(screen.state()),
        2,
        "a piece of news derives the frame once more"
    );
}

#[tokio::test]
async fn a_drawing_a_second_later_derives_nothing_and_says_every_time_a_second_on() {
    let screen = run::opened("work").await;
    let events = logged(&screen);
    let last = events.last().expect("the run's log").timestamp;
    let first = last + chrono::Duration::seconds(SECONDS_AFTER_ITS_LAST_EVENT);
    let second = first + chrono::Duration::from_std(State::TIME_EVERY).expect("in range");
    let mut state = screen.state().clone();
    let read = derived(&state);
    let mut drawn_at = |now: DateTime<Utc>| {
        assert!(
            state.update(Input::Time(now)).is_empty(),
            "the time asks for nothing"
        );
        (times(&drawn(&state)), derived(&state))
    };
    let (before, derived_before) = drawn_at(first);
    let (after, derived_after) = drawn_at(second);
    assert_eq!(
        (derived_before, derived_after),
        (read, read),
        "a drawing at another instant derives nothing"
    );
    assert!(
        before.contains(&gone(&events, first)) && after.contains(&gone(&events, second)),
        "the run's own time is the log's at each drawing: {before:?} then {after:?}"
    );
    assert_eq!(
        before.len(),
        after.len(),
        "the same times: {before:?} {after:?}"
    );
    let moved: Vec<i64> = after.iter().zip(&before).map(|(a, b)| a - b).collect();
    assert!(
        moved.iter().all(|moved| (0..=1).contains(moved)) && moved.contains(&1),
        "every time a second on where it moves with the clock: {before:?} then {after:?}"
    );
}
