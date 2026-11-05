use std::path::{Path, PathBuf};

use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use yunta_core::events::TokenUsage;
use yunta_engine::{RunPhase, RunState};
use yunta_testkit_core::fixed_now;

use super::*;
use crate::run_page::fixtures::{
    exhausted_escalation, failed_lint, finished, frame, lint_failure, outline, parked_on_lint, RUN,
};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/closing")
}

fn closing(frame: RunFrame, decision: Option<(DecisionOn, GateWaitingPayload)>) -> Closing {
    Closing::framed(
        &RUN,
        frame,
        RunState::default(),
        fixed_now(),
        (decision, Vec::new()),
        &outline(),
    )
}

/// The block as a terminal draws it: its page's document, drawn for `look`.
fn draw_closing(closing: &Closing, look: &Look) -> String {
    crate::render::draw(closing.page().document(look), look)
}

#[test]
fn a_finished_run_closes_without_empty_rows() {
    let mut quiet = frame(
        RunPhase::Finished,
        vec![finished("lint"), finished("test")],
        0,
    );
    quiet.tokens = TokenUsage::default();
    let drawn = draw_closing(&closing(quiet, None), &Look::plain());
    for label in ["tokens", "artifacts", "degraded", "unread"] {
        assert!(!drawn.contains(label), "an empty `{label}` row: {drawn}");
    }
}

#[test]
fn a_failed_run_closes_with_the_tail_of_what_failed() {
    let drawn = draw_closing(
        &closing(
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
fn cases() -> Vec<(&'static str, Closing)> {
    let (parked, _) = parked_on_lint();
    let failed = RunPhase::Failed {
        failure: Some(lint_failure()),
    };
    let decision = (
        DecisionOn::Node("lint".into()),
        exhausted_escalation().into_payload(),
    );
    vec![
        (
            "finished",
            closing(
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
            closing(
                frame(failed, vec![failed_lint(), finished("test")], 0),
                None,
            ),
        ),
        (
            "needs-you",
            closing(frame(parked, vec![failed_lint()], 0), Some(decision)),
        ),
        (
            "reported",
            closing(
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
    for (case, closing) in &cases() {
        for environment in &ENVIRONMENTS {
            let look = Look::of(environment);
            assert_golden(
                &environment.golden(&goldens(), case),
                &draw_closing(closing, &look),
            );
        }
    }
}
