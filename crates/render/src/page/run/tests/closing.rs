//! What a run leaves on the terminal once it stops: its page in its closing
//! scope, drawn from the frame and the decision it stopped on.

use std::path::{Path, PathBuf};

use surco::doc::Depth;
use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::events::{DecisionOn, GateWaitingPayload, TokenUsage};
use yunta_core::RunId;
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_replay::RunState;

use super::super::fixtures::{
    closing, failed_lint, finished, frame, lint_decision, lint_failure, parked_on_lint, places, RUN,
};
use super::super::Run;
use crate::escalation::Decision;
use crate::page::Page;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/closing")
}

/// A run stopped on `frame`, and the decision it stopped on.
struct Stopped {
    run_id: RunId,
    frame: RunFrame,
    state: RunState,
    decision: Option<(DecisionOn, GateWaitingPayload)>,
}

impl Stopped {
    /// The closing it leaves: its page at a glance, as the process that
    /// drove it draws it.
    fn page(&self) -> Run<'_> {
        Run {
            scope: closing(&self.frame),
            depth: Depth::Glance,
            run_id: &self.run_id,
            frame: &self.frame,
            state: &self.state,
            places: places(),
            decision: self.decision.as_ref().map(|(on, escalation)| Decision {
                on,
                escalation,
                shown: &[],
            }),
            plans: Vec::new(),
        }
    }
}

fn stopped(frame: RunFrame, decision: Option<(DecisionOn, GateWaitingPayload)>) -> Stopped {
    Stopped {
        run_id: RUN,
        frame,
        state: RunState::default(),
        decision,
    }
}

/// The block as a terminal draws it: its page's document, drawn for `look`.
fn draw_closing(stopped: &Stopped, look: &Look) -> String {
    Terminal::on(*look).draw(&stopped.page().document(look))
}

#[test]
fn a_finished_run_closes_without_empty_rows() {
    let mut quiet = frame(
        RunPhase::Finished,
        vec![finished("lint"), finished("test")],
        0,
    );
    quiet.tokens = TokenUsage::default();
    let drawn = draw_closing(&stopped(quiet, None), &Look::plain());
    for label in ["tokens", "artifacts", "degraded", "unread"] {
        assert!(!drawn.contains(label), "an empty `{label}` row: {drawn}");
    }
}

#[test]
fn a_failed_run_closes_with_the_tail_of_what_failed() {
    let drawn = draw_closing(
        &stopped(
            frame(
                RunPhase::Failed {
                    failure: Some(lint_failure()),
                },
                vec![failed_lint()],
                0,
            ),
            None,
        ),
        &Look::plain(),
    );
    assert!(drawn.contains("node `lint` failed: exit 101."), "{drawn}");
    assert!(
        drawn.contains("error[E0425]: cannot find value `x` in this scope"),
        "{drawn}"
    );
    assert!(
        drawn.contains("whole output: yunta status 7E5PH4 --node lint"),
        "{drawn}"
    );
}

/// The four ways a run closes that a person reads differently: it
/// finished, it failed, it needs them, and it finished with work nobody
/// has accepted.
fn cases() -> Vec<(&'static str, Stopped)> {
    let parked = parked_on_lint();
    let failed = RunPhase::Failed {
        failure: Some(lint_failure()),
    };
    vec![
        (
            "finished",
            stopped(
                frame(
                    RunPhase::Finished,
                    vec![finished("lint"), finished("test")],
                    0,
                ),
                None,
            ),
        ),
        (
            "failed",
            stopped(
                frame(failed, vec![failed_lint(), finished("test")], 0),
                None,
            ),
        ),
        (
            "needs-you",
            stopped(frame(parked, vec![failed_lint()], 0), Some(lint_decision())),
        ),
        (
            "reported",
            stopped(
                frame(
                    RunPhase::Finished,
                    vec![finished("lint"), finished("review")],
                    1,
                ),
                None,
            ),
        ),
    ]
}

#[test]
fn the_closing_block_matches_its_goldens() {
    for (case, stopped) in &cases() {
        for environment in &ENVIRONMENTS {
            let look = Look::of(environment);
            assert_golden(
                &environment.golden(&goldens(), case),
                &draw_closing(stopped, &look),
            );
        }
    }
}
