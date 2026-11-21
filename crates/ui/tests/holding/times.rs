//! See [`super`]. With no event, two drawings of the design's run at work a second apart
//! differ only in their times, and each time is the log's, to the second: how long the run
//! has gone, worked and waited on the person, and how long each node that finished took.

use chrono::{DateTime, Utc};
use surco::line::Line;
use surco::surface::Terminal;
use yunta_core::events::{EventPayload, NodeEvent, RunEvent, StoredEvent};
use yunta_core::NodeId;
use yunta_testkit_ui::screens::run;
use yunta_ui::{Input, State};

use super::{logged, moving, WIDEST};

/// The cells the region beside a run's pane takes on a terminal from 102 columns: its nodes.
const SIDE: usize = 31;

/// The rows a screen's run stands in across it at its tallest: its headline, its ribbon's
/// names and strokes, its gist, and a blank row.
const ACROSS: usize = 6;

/// The rows a run's pane is drawn in where a drawing is compared whole: the design's widest
/// screen's.
const PANE: usize = 26;

/// How long after the design's run's last event at `work` the screen is first drawn: a few
/// seconds, so its ages stand apart from its last event's.
const SECONDS_AFTER_ITS_LAST_EVENT: i64 = 5;

/// `state`'s screen drawn whole as a terminal of 120 columns draws it, its spinner turning as
/// a look that moves turns it: the trail, what stands across, the nodes beside, the pane under
/// its title, the meter, the attention row and the legend, each row's text.
fn drawn_whole(state: &State) -> Vec<String> {
    let screen = state.screen(&moving(WIDEST));
    let terminal = Terminal::on(moving(WIDEST));
    let mut lines = vec![terminal.trail(&screen.trail, &screen.tally)];
    if let Some(about) = &screen.about {
        lines.extend(terminal.across(about, ACROSS));
    }
    let side = state.screen(&moving(SIDE)).side;
    if let Some(side) = side {
        lines.extend(Terminal::on(moving(SIDE)).lines(&side.region.doc));
    }
    lines.push(terminal.title(&screen.main, false));
    lines.extend(terminal.pane(&screen.main, PANE).lines);
    lines.extend(screen.meter.iter().map(|meter| terminal.meter(meter)));
    lines.extend(
        screen
            .attention
            .iter()
            .map(|attention| terminal.attention(attention)),
    );
    lines.push(terminal.legend(&screen.legend, &screen.anywhere));
    lines.iter().map(Line::text).collect()
}

/// The seconds `word` says where it is a time as a screen says one, a figure and its unit
/// once or twice (`9s`, `7m34s`, `2h05m`, `3d04h`); none for any other word.
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

/// Each word of `rows` that says a time, in seconds, least first.
fn times(rows: &[String]) -> Vec<i64> {
    let words = rows
        .iter()
        .flat_map(|row| row.split(|c: char| !c.is_ascii_alphanumeric()));
    let mut times: Vec<i64> = words.filter_map(seconds).collect();
    times.sort_unstable();
    times
}

/// `rows` with each word that says a time in its place written `#`: what stands between two
/// drawings that differ only in their times.
fn times_left_out(rows: &[String]) -> Vec<String> {
    let left_out = |row: &String| {
        let mut out = String::new();
        let mut word = String::new();
        for c in row.chars().chain(std::iter::once(' ')) {
            if c.is_ascii_alphanumeric() {
                word.push(c);
                continue;
            }
            match seconds(&word) {
                Some(_) => out.push('#'),
                None => out.push_str(&word),
            }
            word.clear();
            out.push(c);
        }
        out
    };
    rows.iter().map(left_out).collect()
}

/// The span from `from` to `to`, in seconds.
fn between(from: DateTime<Utc>, to: DateTime<Utc>) -> i64 {
    (to - from).num_seconds()
}

/// Each time the log `events` gives a screen of its run at `now`, in seconds, least first:
/// how long the run has gone; how long it waited on the person, where it did a second or
/// more, and how long it worked, the rest; and how long each node that finished took, its
/// span less the time the run waited on the person within it.
fn times_of_the_log(events: &[StoredEvent], now: DateTime<Utc>) -> Vec<i64> {
    let created = events.iter().find_map(|event| match event.payload()? {
        EventPayload::Run(RunEvent::Created(_)) => Some(event.timestamp),
        _ => None,
    });
    let created = created.expect("the run's log opens with its creation");
    let mut waits: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    let mut started: Vec<(NodeId, DateTime<Utc>)> = Vec::new();
    let mut took = Vec::new();
    for event in events {
        let at = event.timestamp;
        match event.payload() {
            Some(EventPayload::Run(RunEvent::Paused(_))) => waits.push((at, now)),
            Some(EventPayload::Run(RunEvent::Resumed(_))) => {
                if let Some(wait) = waits.last_mut() {
                    wait.1 = at;
                }
            }
            Some(EventPayload::Node(NodeEvent::Started(_))) => {
                let node = event.node_id.clone().expect("a node's start names it");
                started.push((node, at));
            }
            Some(EventPayload::Node(NodeEvent::Finished(_))) => {
                let node = event.node_id.as_ref().expect("a node's end names it");
                let from = started.iter().rev().find(|(started, _)| started == node);
                let from = from.expect("a node ends once it started").1;
                let waited: i64 = waits
                    .iter()
                    .map(|(paused, resumed)| between(*paused.max(&from), *resumed.min(&at)).max(0))
                    .sum();
                took.push(between(from, at) - waited);
            }
            _ => {}
        }
    }
    let gone = between(created, now);
    let waited: i64 = waits
        .iter()
        .map(|(paused, resumed)| between(*paused, *resumed))
        .sum();
    let mut times = vec![gone, gone - waited];
    times.extend((waited > 0).then_some(waited));
    times.extend(took);
    times.sort_unstable();
    times
}

#[tokio::test]
async fn with_no_event_two_drawings_a_second_apart_differ_only_in_times_each_the_logs() {
    let screen = run::opened("work").await;
    let events = logged(&screen);
    let last = events.last().expect("the run's log").timestamp;
    let first = last + chrono::Duration::seconds(SECONDS_AFTER_ITS_LAST_EVENT);
    let every = chrono::Duration::from_std(State::TIME_EVERY).expect("a cadence in range");
    let mut state = screen.state().clone();

    let mut drawn_at = |now: DateTime<Utc>| {
        let asked = state.update(Input::Time(now));
        assert!(asked.is_empty(), "the time asks for nothing: {asked:?}");
        drawn_whole(&state)
    };
    let before = drawn_at(first);
    let after = drawn_at(first + every);
    assert_ne!(before, after, "the times move with the clock");
    assert_eq!(
        times_left_out(&after),
        times_left_out(&before),
        "nothing but the times moved"
    );
    assert_eq!(
        times(&before),
        times_of_the_log(&events, first),
        "each time on the screen is the log's: {before:#?}"
    );
    assert_eq!(
        times(&after),
        times_of_the_log(&events, first + every),
        "each time on the screen is the log's a second later: {after:#?}"
    );
}
