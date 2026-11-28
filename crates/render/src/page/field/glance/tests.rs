//! The run the field points at, at a glance, as the field's side region at 120 columns draws
//! it: for a run that needs the person, one stalled, one stopped, one asking at its terminal
//! and one that finished, the facts and the acts the field's targets carry, each act taken by
//! the key the field binds to its step. Every expected row is written by hand.

use std::sync::Arc;
use std::time::Duration;

use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::surface::{Surface, Terminal};
use surco::words::Standing;
use yunta_core::events::PauseReason;
use yunta_core::step::NextStep;
use yunta_core::{Isolation, NodeId, Pid, RunId};
use yunta_replay::frame::{RunPhase, RunTimes, WaitingOn};
use yunta_replay::registry::{Engine, Prompt};
use yunta_testkit_core::fixed_now;

use super::*;
use crate::page::run::fixtures::{
    ahead, failed_lint, finished, frame, lint_decision, parked_on_lint, times,
};
use crate::page::RunUnread;

/// The run `id` at `phase`, its nodes `nodes`, called `word`, where it has stood `stood`.
fn row(
    id: &'static str,
    phase: RunPhase,
    nodes: Vec<yunta_replay::frame::NodeFrame>,
    word: Standing,
    stood: Duration,
) -> FieldRow {
    let mut frame = frame(phase, nodes, 0);
    frame.run_id = RunId::from_static(id);
    frame.standing = word;
    let times = RunTimes {
        since_last_event: Some(stood),
        ..times(&frame)
    };
    FieldRow {
        frame: Arc::new(frame),
        times,
        menu: false,
    }
}

/// What the field offers on `row`, `decision` the decision it waits on: each act the run offers
/// that a key of the field takes, by that key (`r` going on, `x` closing, `c` stopping).
fn offered(
    row: &FieldRow,
    decision: Option<&yunta_core::events::GateWaitingPayload>,
) -> Vec<(String, GlanceAct)> {
    row.frame
        .actions(decision)
        .into_iter()
        .filter_map(|action| {
            let key = match action.step {
                NextStep::Resume { .. } => "r",
                NextStep::Close { .. } => "x",
                NextStep::Cancel { .. } => "c",
                _ => return None,
            };
            Some((key.to_string(), GlanceAct::Offered(action.effect)))
        })
        .collect()
}

/// `glance` as the field's side region at 120 columns draws it: 39 cells, unpainted, in
/// Unicode, a row a string.
fn drawn(glance: &RunAtAGlance<'_>) -> Vec<String> {
    let look = Look {
        glyphs: Glyphs::Unicode,
        width: Width::exact(39),
        ..Look::plain()
    };
    Terminal::on(look)
        .draw(&glance.document(&look))
        .lines()
        .map(str::to_string)
        .collect()
}

/// `glance`'s rows from its first fact on: its facts, then what can be done.
fn from_its_facts(glance: &RunAtAGlance<'_>) -> Vec<String> {
    let rows = drawn(glance);
    let first = rows
        .iter()
        .position(|row| row.starts_with("  ") && row.trim_start().contains("  "))
        .unwrap_or_else(|| panic!("no fact: {rows:#?}"));
    rows[first..].to_vec()
}

/// `row` at a glance, as the field reads it, offering `acts`.
fn glance<'a>(row: &'a FieldRow, acts: &'a [(String, GlanceAct)]) -> RunAtAGlance<'a> {
    RunAtAGlance::Read {
        row,
        base_branch: "main",
        isolation: Isolation::Worktree,
        acts,
    }
}

const WAITS: &str = "01M7SMA11RVN00000000EZRPE9";
const STALLS: &str = "01M7SMA11RVN00000000EKPBGE";
const STOPS: &str = "01M7SMA11RVN00000000ST0PPD";
const ASKS: &str = "01M7DES1GNED00000000Q78GRR";
const ENDED: &str = "01M7SMA11RVN00000000REP0RT";

/// A run that waits on a decision says how long it has waited and its branch, then opens on
/// `enter` where the decision is read and closes on `x`, as `field-marked` draws it.
#[test]
fn a_run_that_needs_you_offers_its_decision_and_its_close() {
    let waiting = row(
        WAITS,
        parked_on_lint(),
        vec![finished("plan"), failed_lint(), ahead("ship")],
        Standing::NeedsYou,
        Duration::from_secs(40 * 86_400),
    );
    let (_, decision) = lint_decision();
    let mut acts = vec![("enter".to_string(), GlanceAct::ReadsTheDecision)];
    acts.extend(offered(&waiting, Some(&decision)));
    assert_eq!(
        from_its_facts(&glance(&waiting, &acts)),
        [
            "  waiting      for 40d",
            "  branch       yunta/run/...EZRPE9 off",
            "               main",
            "",
            "what you can do",
            "enter  read the decision",
            "x      close it for good",
        ]
    );
}

/// A stalled run is continued from its log or closed for good, as `field-glance-stalled`
/// draws it; a stopped one is continued where it stopped, as `field-glance-stopped` does.
#[test]
fn a_stalled_and_a_stopped_run_say_how_each_goes_on() {
    let stalled = row(
        STALLS,
        RunPhase::Running,
        vec![finished("plan"), ahead("ship")],
        Standing::Stalled,
        Duration::from_secs(11 * 86_400),
    );
    let stalls = offered(&stalled, None);
    assert_eq!(
        from_its_facts(&glance(&stalled, &stalls)),
        [
            "  waiting      for 11d",
            "  branch       yunta/run/...EKPBGE off",
            "               main",
            "",
            "what you can do",
            "r      continue it from its log",
            "x      close it for good",
        ]
    );
    let cancelled = RunPhase::Waiting {
        on: WaitingOn::Run {
            reason: PauseReason::Cancelled,
        },
    };
    let stopped = row(
        STOPS,
        cancelled,
        vec![finished("plan")],
        Standing::Stopped,
        Duration::from_secs(3_600),
    );
    let stops = offered(&stopped, None);
    let rows = from_its_facts(&glance(&stopped, &stops));
    assert_eq!(rows[0], "  waiting      for 1h");
    assert_eq!(
        rows[rows.len() - 2..],
        [
            "r      continue it where it stopped",
            "x      close it for good"
        ]
    );
}

/// A run asking about `tests` at its terminal, two of its three nodes done after a reroute,
/// at work 1h12m and waiting on a person 38m51s, as the design's run stands in the field.
fn asking() -> FieldRow {
    let mut asking = row(
        ASKS,
        RunPhase::Running,
        vec![finished("plan"), finished("implement"), ahead("ship")],
        Standing::NeedsYou,
        Duration::ZERO,
    );
    let engine = Engine::Alive {
        pid: Pid::current(),
        prompt: Some(Prompt {
            node: Some(NodeId::from("tests")),
            since: fixed_now(),
        }),
        words: None,
    };
    asking.times.asking = engine.asking(fixed_now());
    asking.times.time.working = Duration::from_secs(4_326);
    asking.times.time.people = Duration::from_secs(2_331);
    Arc::make_mut(&mut asking.frame).reroutes = 1;
    asking
}

/// The labels of the facts among `rows`, in their order.
fn labels(rows: &[String]) -> Vec<String> {
    rows.iter()
        .filter_map(|row| row.strip_prefix("  "))
        .filter_map(|row| row.split("  ").next())
        .filter(|label| !label.is_empty() && !label.contains(' ') && !label.contains('`'))
        .map(str::to_string)
        .collect()
}

/// A run asking at its terminal says what holds it as its row does, how far it has come, what
/// it spent, where its time went, its slowest nodes and its branch, and offers to stop it and
/// what it cost, as `field` draws it.
#[test]
fn a_run_at_work_says_how_far_it_came_and_what_it_spent() {
    let asking = asking();
    let mut acts = offered(&asking, None);
    acts.push(("s".to_string(), GlanceAct::ShowsWhatItCost));
    let rows = drawn(&glance(&asking, &acts));
    assert!(
        rows[0].starts_with("◆ fragua (quick) · needs you") && rows[0].ends_with("Q78GRR"),
        "{rows:#?}"
    );
    assert!(
        rows.contains(&"  node `tests` asks at its terminal.".to_string()),
        "{rows:#?}"
    );
    assert_eq!(
        labels(&rows),
        ["progress", "tokens", "time", "slowest", "branch"],
        "{rows:#?}"
    );
    for said in [
        "  progress     2 of 3 nodes done, 1",
        "               reroute",
        "  time         1h12m at work, 38m51s",
        "               waiting on you",
    ] {
        assert!(rows.contains(&said.to_string()), "{said}: {rows:#?}");
    }
    assert_eq!(
        rows[rows.len() - 2..],
        [
            "c      stop it and everything under it",
            "s      what it cost"
        ]
    );
}

/// A run that finished says how far it came, what it spent, its slowest nodes and its branch,
/// and where no key of the field takes what it offers, the glance offers nothing.
#[test]
fn a_run_that_ended_says_the_same_facts_and_offers_no_key() {
    let ended = row(
        ENDED,
        RunPhase::Finished,
        vec![finished("plan")],
        Standing::Finished,
        Duration::from_secs(3_600),
    );
    let acts = offered(&ended, None);
    let rows = drawn(&glance(&ended, &acts));
    assert_eq!(
        labels(&rows),
        ["progress", "tokens", "slowest", "branch"],
        "{rows:#?}"
    );
    assert!(
        !rows.iter().any(|row| row == crate::screen::WHAT_YOU_CAN_DO),
        "{rows:#?}"
    );
}

/// Below the width the glance stands at, its acts stand alone under their heading.
#[test]
fn its_acts_stand_alone_where_the_glance_gives_way() {
    let stalled = row(
        STALLS,
        RunPhase::Running,
        vec![finished("plan")],
        Standing::Stalled,
        Duration::from_secs(60),
    );
    let acts = offered(&stalled, None);
    let alone = glance(&stalled, &acts)
        .what_you_can_do()
        .expect("a stalled run offers what it can do");
    let look = Look {
        glyphs: Glyphs::Unicode,
        width: Width::exact(56),
        ..Look::plain()
    };
    let rows: Vec<String> = Terminal::on(look)
        .block(&alone)
        .iter()
        .map(|line| line.text())
        .collect();
    assert_eq!(
        rows,
        [
            "what you can do",
            "r      continue it from its log",
            "x      close it for good",
        ]
    );
}

/// A run the field cannot derive is named, with why, and offers nothing.
#[test]
fn an_unreadable_run_says_why_and_offers_nothing() {
    let unread = UnreadableRun::new(
        &RunId::from_static("01M7SMA11RVN000000000RPHAN"),
        RunUnread::Manifest {
            path: "/cut/runs/01M7SMA11RVN000000000RPHAN/manifest.yaml".into(),
        },
    );
    let glance = RunAtAGlance::Unread(&unread);
    let rows = drawn(&glance).join("\n");
    assert!(
        rows.contains("unreadable") && rows.contains("manifest missing or unreadable at"),
        "{rows}"
    );
    assert!(glance.what_you_can_do().is_none());
}
