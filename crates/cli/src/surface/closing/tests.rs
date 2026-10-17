use std::path::{Path, PathBuf};

use yunta_core::events::{
    Claim, CommandExit, Escalation, ExhaustedReroute, Failure, FailureMenu, GateOption, Menu,
    NodeOutcome, NodeState, NodeWait, PauseReason, TokenUsage,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{ContentHash, NonEmpty};
use yunta_engine::{Counter, NodeStanding, WaitingOn};
use yunta_testkit::{assert_golden, node_frame, run_frame, ENVIRONMENTS};
use yunta_testkit_core::fixed_now;

use super::*;

const RUN: RunId = RunId::from_static("01K3W48MFW7H0ZZA5PZ07E5PH4");

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/closing")
}

fn outline() -> Outline<'static> {
    Outline {
        run_dir: Path::new("/home/me/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4"),
        worktree: Path::new("/home/me/.yunta/worktrees/01K3W48MFW7H0ZZA5PZ07E5PH4"),
        base_branch: "main",
        isolation: Isolation::Worktree,
        cwd: Path::new("/home/me/project"),
        home: Some(Path::new("/home/me")),
    }
}

fn reached(id: &str, state: NodeState) -> NodeFrame {
    node_frame(&NodeId::from(id), NodeStanding::Reached(state))
}

fn finished(id: &str) -> NodeFrame {
    reached(
        id,
        NodeState::Finished {
            outcome: NodeOutcome::Summary("exit 0".to_string()),
            tokens: TokenUsage::default(),
        },
    )
}

/// How `lint` fails, as a compiler does: what went wrong on stdout,
/// kept whole in the run's objects.
fn lint_failure() -> Failure {
    Failure::exited(CommandExit {
        origin: None,
        code: 101,
        tail: vec![
            "error[E0425]: cannot find value `x` in this scope".to_string(),
            " --> src/lib.rs:3:5".to_string(),
        ],
        output: Some(ContentHash::sha256(b"what lint printed")),
    })
}

fn failed_lint() -> NodeFrame {
    reached(
        "lint",
        NodeState::Failed {
            failure: lint_failure(),
            tokens: TokenUsage::default(),
        },
    )
}

fn frame(phase: RunPhase, nodes: Vec<NodeFrame>) -> RunFrame {
    let failed = nodes
        .iter()
        .filter(|node| matches!(node.state, NodeStanding::Reached(NodeState::Failed { .. })))
        .count();
    RunFrame {
        phase,
        flow: Counter {
            total: nodes.len(),
            done: nodes.len() - failed,
            failed,
            ..Counter::default()
        },
        tokens: TokenUsage {
            input: 18_000,
            output: 2_200,
            cached: None,
        },
        nodes,
        ..run_frame(&RUN)
    }
}

/// The escalation a node parks on once its re-routes are spent: the failure
/// of `lint`, whose re-route to `fix-lint` allows no further attempt.
fn exhausted_escalation() -> Escalation {
    let menu = FailureMenu {
        attempt: 1,
        failure: lint_failure(),
        remedies: Vec::new(),
        stuck: None,
        reroute: Some(ExhaustedReroute {
            goto: NodeId::from("fix-lint"),
            max_reroutes: 0,
            next_mode: None,
        }),
    };
    Escalation::new(
        Claim::NodeFailed {
            node: NodeId::from("lint"),
        },
        Menu::Failure(Box::new(menu)),
        NonEmpty::from((
            GateOption::reserved(ReservedOption::Retry),
            vec![GateOption::reserved(ReservedOption::Abort)],
        )),
    )
}

fn exhausted() -> GateWaitingPayload {
    exhausted_escalation().into_payload()
}

fn closing(frame: RunFrame, decision: Option<(NodeId, GateWaitingPayload)>) -> Closing {
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
    crate::render::draw(closing.page(look).document(look), look)
}

#[test]
fn a_finished_run_closes_without_empty_rows() {
    let mut quiet = frame(RunPhase::Finished, vec![finished("lint"), finished("test")]);
    quiet.tokens = TokenUsage::default();
    let drawn = draw_closing(&closing(quiet, None), &Look::plain());
    for label in ["tokens", "artifacts", "degraded", "unread"] {
        assert!(!drawn.contains(label), "an empty `{label}` row: {drawn}");
    }
}

#[test]
fn a_failed_run_closes_with_the_tail_of_what_failed() {
    let failure = lint_failure();
    let drawn = draw_closing(
        &closing(
            frame(
                RunPhase::Failed {
                    failure: Some(failure),
                },
                vec![failed_lint()],
            ),
            None,
        ),
        &Look::plain(),
    );
    assert!(drawn.contains("node `lint` failed: exit 101"), "{drawn}");
    assert!(
        drawn.contains("error[E0425]: cannot find value `x` in this scope"),
        "{drawn}"
    );
    assert!(
        drawn.contains("whole output: ~/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4/objects/"),
        "{drawn}"
    );
}

/// The four ways a run closes that a person reads differently: it
/// finished, it failed, it needs them, and it finished with work nobody
/// has accepted.
fn cases() -> Vec<(&'static str, Closing)> {
    let failure = lint_failure();
    let parked = RunPhase::Waiting {
        on: WaitingOn::Node {
            node: NodeId::from("lint"),
            on: NodeWait::Gate { external_ref: None },
            reason: Some(PauseReason::Escalation(Box::new(exhausted_escalation()))),
        },
    };
    let mut reported = frame(
        RunPhase::Finished,
        vec![finished("lint"), finished("review")],
    );
    reported.blocking_findings = 1;
    let failed = RunPhase::Failed {
        failure: Some(failure),
    };
    vec![
        (
            "finished",
            closing(
                frame(RunPhase::Finished, vec![finished("lint"), finished("test")]),
                None,
            ),
        ),
        (
            "failed",
            closing(frame(failed, vec![failed_lint(), finished("test")]), None),
        ),
        (
            "needs-you",
            closing(
                frame(parked, vec![failed_lint()]),
                Some((NodeId::from("lint"), exhausted())),
            ),
        ),
        ("reported", closing(reported, None)),
    ]
}

#[test]
fn the_closing_block_matches_its_goldens() {
    for (case, closing) in &cases() {
        for environment in &ENVIRONMENTS {
            let look = crate::render::look_of(environment);
            assert_golden(
                &environment.golden(&goldens(), case),
                &draw_closing(closing, &look),
            );
        }
    }
}
