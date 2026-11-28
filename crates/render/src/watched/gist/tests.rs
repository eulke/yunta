//! The row that says what a watched run comes to.

use surco::line::Tone;
use surco::paint::Paint;
use surco::words::Standing;
use yunta_core::events::{NodeWait, PauseReason};
use yunta_core::{NodeId, RunId};
use yunta_replay::frame::{RunFrame, RunPhase, RunTimes, WaitingOn};

use super::gist;

const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

/// A run in `phase`, called as its frame calls a run there.
fn standing(phase: RunPhase) -> RunFrame {
    RunFrame {
        standing: yunta_replay::frame::run_standing(
            &phase,
            &[],
            0,
            &yunta_replay::registry::Engine::this(yunta_core::Pid::current()),
        ),
        phase,
        ..RunFrame::sample(&RUN)
    }
}

/// The row's words and the tone they are said in.
fn said(frame: &RunFrame) -> Option<(String, Vec<Tone>)> {
    gist(frame, &RunTimes::sample(frame)).map(|row| {
        let tones = row.spans().iter().map(|span| span.tone).collect();
        (Paint::Plain.line(&row), tones)
    })
}

#[test]
fn a_moving_run_says_nothing_needs_you_muted() {
    assert_eq!(
        said(&standing(RunPhase::Running)),
        Some((
            "nothing needs you: it is moving".to_string(),
            vec![Tone::Muted]
        ))
    );
}

/// A run parked on a gate says what it waits on in the person's tone: the words its
/// page's gist says.
#[test]
fn a_run_that_needs_you_says_what_it_waits_on_in_your_tone() {
    let parked = standing(RunPhase::Waiting {
        on: WaitingOn::Node {
            node: NodeId::from_static("plan"),
            on: NodeWait::Gate { external_ref: None },
            reason: None,
        },
    });
    assert_eq!(parked.standing, Standing::NeedsYou);
    let (words, tones) = said(&parked).expect("a parked run's gist");
    assert_eq!(
        Some(words),
        crate::state::gist(&parked, &RunTimes::sample(&parked))
    );
    assert_eq!(tones, [Tone::NeedsYou]);
}

/// A run waiting on a limit's reset needs nobody: it says so, muted.
#[test]
fn a_run_waiting_on_a_reset_says_it_muted() {
    let reset = standing(RunPhase::Waiting {
        on: WaitingOn::Run {
            reason: PauseReason::AwaitingReset {
                node: NodeId::from_static("build"),
                at: yunta_testkit_core::fixed_now(),
            },
        },
    });
    let (words, tones) = said(&reset).expect("a waiting run's gist");
    assert_eq!(
        Some(words),
        crate::state::gist(&reset, &RunTimes::sample(&reset))
    );
    assert_eq!(tones, [Tone::Muted]);
}

/// A run whose word says it all has no row to say more.
#[test]
fn a_finished_run_has_no_row() {
    assert_eq!(said(&standing(RunPhase::Finished)), None);
}
