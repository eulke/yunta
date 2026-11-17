//! What a watched run has spent, as its meter says it.

use std::time::Duration;

use surco::glyphs::Glyphs;
use surco::look::Look;
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_core::events::TokenUsage;
use yunta_core::{NodeId, RunId};
use yunta_replay::frame::{NodeFrame, NodeStanding, RunFrame};
use yunta_replay::TimeSpent;

use super::spent;

const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

/// `input` tokens in and none out.
fn tokens(input: u64) -> TokenUsage {
    TokenUsage {
        input,
        output: 0,
        cached: None,
    }
}

/// A node that spent `input` tokens.
fn node(id: &'static str, input: u64) -> NodeFrame {
    NodeFrame {
        tokens: tokens(input),
        ..NodeFrame::sample(&NodeId::from_static(id), NodeStanding::ToGo)
    }
}

/// A run of three nodes that spent 20.2k tokens between two of them, at work for
/// `working` seconds and waiting on the person for `people` milliseconds.
fn run(working: u64, people: u64) -> RunFrame {
    RunFrame {
        tokens: TokenUsage {
            input: 18_000,
            output: 2_200,
            cached: Some(9_000),
        },
        nodes: vec![node("plan", 18_000), node("build", 2_200), node("ship", 0)],
        time: TimeSpent {
            working: Duration::from_secs(working),
            people: Duration::from_millis(people),
            ..TimeSpent::default()
        },
        ..RunFrame::sample(&RUN)
    }
}

/// The times a meter says, each as its figure, its label and whether it is the person's.
fn times(frame: &RunFrame) -> Vec<(String, String, bool)> {
    spent(frame)
        .times
        .into_iter()
        .map(|time| (time.figure, time.label, time.by_person))
        .collect()
}

#[test]
fn the_meter_says_the_runs_tokens_a_bar_a_node_and_its_time_at_work() {
    let meter = spent(&run(154, 0));
    assert_eq!(
        meter.count, "20.2k tokens",
        "in and out, the cache no part of it"
    );
    assert_eq!(meter.bars, [18_000.0, 2_200.0, 0.0]);
    assert_eq!(
        times(&run(154, 0)),
        [("2m34s".to_string(), "at work".to_string(), false)]
    );
}

#[test]
fn the_time_waiting_on_the_person_is_said_where_it_comes_to_a_second() {
    assert_eq!(
        times(&run(154, 300_000)),
        [
            ("2m34s".to_string(), "at work".to_string(), false),
            ("5m00s".to_string(), "waiting on you".to_string(), true),
        ]
    );
    assert_eq!(times(&run(154, 400)).len(), 1, "under a second is no part");
}

/// A run that has spent nothing yet still says so: no tokens, and no time at work.
#[test]
fn a_run_that_spent_nothing_says_it_spent_nothing() {
    let frame = RunFrame {
        nodes: vec![node("plan", 0)],
        ..RunFrame::sample(&RUN)
    };
    let meter = spent(&frame);
    assert_eq!(meter.count, "0 tokens");
    assert_eq!(
        times(&frame),
        [("0s".to_string(), "at work".to_string(), false)]
    );
}

/// On a terminal the meter is one row: the figure, its bars, and the times after the
/// separator.
#[test]
fn a_terminal_draws_the_meter_as_one_row() {
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: surco::look::Width::row(120, None),
        motion: Motion::Still,
    };
    let row = Terminal::on(look).meter(&spent(&run(154, 300_000)));
    assert_eq!(
        row.text(),
        "20.2k tokens █▂▁ · 2m34s at work · 5m00s waiting on you"
    );
}
