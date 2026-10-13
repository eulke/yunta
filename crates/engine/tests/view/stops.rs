//! What the frame says where a run stops: a parked node carries the sentence
//! its pause recorded, a run that moves again quotes no pause, a paused run
//! with no parked node names the reason the log recorded, a failed run carries
//! its failure, and a promoted run names the mode its signal suggested.

use super::*;

#[test]
fn a_parked_node_carries_the_sentence_its_own_pause_recorded() {
    // A node id names *which* node stopped; only the sentence the engine
    // wrote when it stopped the run names *what* that node asked for. A
    // surface that had to read the log again for it could print a
    // sentence the frame beside it does not agree with.
    let workflow = chain();
    let asked = "node `plan` asked 1 question: `what-changed`";
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), gate_waiting(None)),
        (
            3,
            None,
            EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
                &PauseReason::Recorded {
                    sentence: asked.to_string(),
                },
            ))),
        ),
    ]);

    assert_eq!(
        frame(&workflow, &events, 100).phase,
        RunPhase::Waiting {
            on: WaitingOn::Node {
                node: "plan".into(),
                on: NodeWait::Gate { external_ref: None },
                reason: Some(PauseReason::Recorded {
                    sentence: asked.to_string(),
                }),
            }
        }
    );
}

#[test]
fn a_node_parked_while_the_run_moves_again_quotes_no_pause() {
    // The run resumed after its last pause, so that sentence describes a
    // stop the run has already left behind. The node is still parked and
    // the frame says so, quoting nothing rather than the wrong thing.
    let workflow = chain();
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), gate_waiting(None)),
        (
            3,
            None,
            EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
                &PauseReason::Recorded {
                    sentence: "token budget exceeded".to_string(),
                },
            ))),
        ),
        (
            4,
            None,
            EventPayload::Run(RunEvent::Resumed(RunResumedPayload {
                checkout: None,
                policies: Vec::new(),
                environment: None,
                tree: None,
            })),
        ),
        (5, Some("build"), started(1)),
    ]);

    assert_eq!(
        frame(&workflow, &events, 100).phase,
        RunPhase::Waiting {
            on: WaitingOn::Node {
                node: "plan".into(),
                on: NodeWait::Gate { external_ref: None },
                reason: None,
            }
        }
    );
}

#[test]
fn a_paused_run_with_no_parked_node_names_the_reason_the_log_recorded() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("build"), started(1)),
        (2, Some("build"), failed("out of budget")),
        (
            3,
            None,
            EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
                &PauseReason::Recorded {
                    sentence: "token budget exceeded".to_string(),
                },
            ))),
        ),
    ]);

    assert_eq!(
        frame(&workflow, &events, 100).phase,
        RunPhase::Waiting {
            on: WaitingOn::Run {
                reason: PauseReason::Recorded {
                    sentence: "token budget exceeded".to_string(),
                },
            }
        }
    );
}

#[test]
fn a_failed_run_carries_the_failure_its_last_node_failed_recorded() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("build"), started(1)),
        (2, Some("build"), failed("criteria still red")),
        (3, None, run_finished(TerminalState::Failed)),
    ]);

    assert_eq!(
        frame(&workflow, &events, 100).phase,
        RunPhase::Failed {
            failure: Some(Failure::message("criteria still red".to_string())),
        }
    );
}

#[test]
fn a_promoted_run_names_the_mode_its_signal_suggested() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("build"), started(1)),
        (2, Some("build"), finished()),
        (
            3,
            None,
            EventPayload::Run(RunEvent::PromotionSignaled(PromotionSignaledPayload::new(
                PromotionCause {
                    node: "build".into(),
                    goto: "fix".into(),
                    failure: Some(Failure::message("criteria still red")),
                },
                vec![Fact::bare("criteria still red")].into(),
                "beta".into(),
            ))),
        ),
        (4, None, run_finished(TerminalState::Promoted)),
    ]);

    assert_eq!(
        frame(&workflow, &events, 100).phase,
        RunPhase::Promoted {
            to: Some("beta".into()),
        }
    );
}
