//! A run stopped by its token cap, answered from another process: the answer waits on the
//! log for the engine that takes it, and one answer pays for one engine.

use yunta_core::events::DecisionOn;
use yunta_core::events::{
    EventDraft, EventPayload, GateEvent, GateResolvedPayload, HumanChoice, NodeEvent, PauseReason,
    RunEvent, RunPausedPayload, RunResumedPayload,
};
use yunta_core::refusal::GateRefusal;
use yunta_engine::{awaits_decision, current_escalation, RunTerminal};
use yunta_replay::RunState;
use yunta_testkit::Bench;
use yunta_testkit_core::{FixedClock, Log};

mod common;
use common::*;

/// A run parked on its cap with no terminal to ask at.
async fn parked_on_the_cap() -> Bench {
    let bench = Bench::new();
    let first = bench
        .run_with_config(BUDGET_WORKFLOW, BUDGET_FIXTURE, BUDGET_CONFIG)
        .await;
    assert!(
        matches!(
            first.terminal,
            RunTerminal::Paused {
                reason: PauseReason::BudgetExhausted { .. }
            }
        ),
        "{:?}",
        first.terminal
    );
    bench
}

fn on_the_cap() -> EventPayload {
    EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
        &PauseReason::BudgetExhausted { spent: 5, cap: 4 },
    )))
}

fn on_something_else() -> EventPayload {
    EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
        &PauseReason::Recorded {
            sentence: "parked".into(),
        },
    )))
}

/// An answer with no node, which is how the run's own decision is recorded.
fn answered() -> EventPayload {
    EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(
        HumanChoice {
            option: "continue".into(),
            by: "mcp".into(),
            free_text: None,
        },
    )))
}

fn resumed() -> EventPayload {
    EventPayload::Run(RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None)))
}

/// Whether a log of `events` holds an answer an engine has yet to take.
fn waits(events: Vec<EventPayload>) -> bool {
    let log = events
        .into_iter()
        .fold(Log::for_run("run-1"), |log, event| log.event(event));
    RunState::replay(&log.build()).answer_awaits_its_resume()
}

#[tokio::test]
async fn a_run_on_the_cap_offers_its_own_decision() {
    let bench = parked_on_the_cap().await;
    let state = RunState::replay(&bench.events());
    let (on, _) = current_escalation(&bench.manifest(), &bench.run_dir(), &state)
        .await
        .unwrap()
        .expect("the cap is a pause with a menu");
    assert_eq!(on, DecisionOn::Run);
    assert_eq!(
        awaits_decision(&bench.manifest(), &state),
        Some(DecisionOn::Run)
    );
}

/// Between an answer and the engine that takes it the run is on its way, and what was
/// answered is on offer to nobody.
#[tokio::test]
async fn an_answer_waits_for_the_engine_that_takes_it() {
    let bench = parked_on_the_cap().await;

    answer_parked(&bench, "continue")
        .await
        .expect("the cap's menu offers continue");

    let state = RunState::replay(&bench.events());
    assert!(state.answer_awaits_its_resume());
    assert!(
        current_escalation(&bench.manifest(), &bench.run_dir(), &state)
            .await
            .unwrap()
            .is_none(),
        "nothing is on offer"
    );
    assert_eq!(awaits_decision(&bench.manifest(), &state), None);
}

#[tokio::test]
async fn a_second_answer_is_refused_and_records_nothing() {
    let bench = parked_on_the_cap().await;
    answer_parked(&bench, "continue").await.unwrap();

    assert!(answer_parked(&bench, "abort").await.is_err());

    let answers = bench
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Gates(GateEvent::Resolved(_)))
            )
        })
        .count();
    assert_eq!(answers, 1);
}

#[tokio::test]
async fn an_option_off_the_menu_is_refused() {
    let bench = parked_on_the_cap().await;
    assert!(matches!(
        answer_parked(&bench, "retry").await,
        Err(yunta_engine::ResolveGateError::Refused(
            GateRefusal::UnknownOption { .. }
        ))
    ));
}

/// The window of the answer: right after the cap's pause, with nothing having taken it.
#[test]
fn an_answer_counts_only_after_the_cap_s_pause_and_before_a_resume() {
    assert!(waits(vec![on_the_cap(), answered()]));

    // A terminal's abort: the answer comes first, and the pause is what it did.
    assert!(!waits(vec![answered(), on_the_cap()]));
    // A terminal's continue on a wake, and a later stop on the cap.
    assert!(!waits(vec![
        on_the_cap(),
        resumed(),
        answered(),
        on_the_cap()
    ]));
    // An engine took it.
    assert!(!waits(vec![on_the_cap(), answered(), resumed()]));
    // The run stands on something else, whose answers name a node.
    assert!(!waits(vec![on_something_else(), answered()]));
}

/// With an answer waiting there is nothing to ask: a surface that would say `abort` is
/// never asked, and the run goes on.
#[tokio::test]
async fn an_engine_with_an_answer_waiting_takes_it_and_does_not_ask() {
    let bench = parked_on_the_cap().await;
    answer_parked(&bench, "continue").await.unwrap();

    let woken = bench
        .wake_answering(&SequencedInteraction::choosing(&["abort"]))
        .await;

    assert_eq!(woken.terminal, RunTerminal::Finished);
}

/// An `abort` that waits is taken the same way: it stops the run where it stood, with
/// nobody asked and nothing spent.
#[tokio::test]
async fn an_abort_waiting_stops_the_run_where_it_stood() {
    let bench = parked_on_the_cap().await;
    answer_parked(&bench, "abort").await.unwrap();

    let woken = bench
        .wake_answering(&SequencedInteraction::choosing(&["continue"]))
        .await;

    assert!(
        matches!(
            woken.terminal,
            RunTerminal::Paused {
                reason: PauseReason::BudgetExhausted { .. }
            }
        ),
        "{:?}",
        woken.terminal
    );
    assert!(
        current_escalation(
            &bench.manifest(),
            &bench.run_dir(),
            &RunState::replay(&bench.events())
        )
        .await
        .unwrap()
        .is_some(),
        "the decision stands open"
    );
}

/// The workflow of [`BUDGET_WORKFLOW`], then a gate, then a node that spends: the engine
/// that took an answer parks on the gate, and the one the gate's answer wakes owes its own.
fn budget_then_a_gate() -> String {
    format!(
        "{BUDGET_WORKFLOW}  - {{ id: check, kind: gate, depends_on: [first, fix], assignee: lead }}\n  \
         - {{ id: last, kind: prompt, runner: executor, depends_on: [check], prompt: \"One more.\" }}\n"
    )
}

/// One answer pays for one engine, whatever that engine went on to do.
#[tokio::test]
async fn the_engine_after_the_one_that_took_the_answer_stops_on_the_cap() {
    let bench = Bench::new();
    let first = bench
        .run_with_config(&budget_then_a_gate(), BUDGET_FIXTURE, BUDGET_CONFIG)
        .await;
    assert!(
        matches!(
            first.terminal,
            RunTerminal::Paused {
                reason: PauseReason::BudgetExhausted { .. }
            }
        ),
        "{:?}",
        first.terminal
    );

    answer_parked(&bench, "continue").await.unwrap();
    let on_the_gate = bench.wake().await;
    assert!(
        matches!(on_the_gate.terminal, RunTerminal::Paused { .. }),
        "{:?}",
        on_the_gate.terminal
    );
    assert!(
        !matches!(
            on_the_gate.terminal,
            RunTerminal::Paused {
                reason: PauseReason::BudgetExhausted { .. }
            }
        ),
        "the answer took it past the cap: {:?}",
        on_the_gate.terminal
    );

    answer_parked(&bench, "approve").await.unwrap();
    let next = bench.wake().await;
    assert!(
        matches!(
            next.terminal,
            RunTerminal::Paused {
                reason: PauseReason::BudgetExhausted { .. }
            }
        ),
        "{:?}",
        next.terminal
    );
}

/// Two nodes under a cap of 100, whose sessions go past their share of it, as real ones do.
const PAST_ITS_SHARE: &str = r#"
name: past-its-share
nodes:
  - { id: first, kind: prompt, runner: executor, prompt: "Do the first thing." }
  - { id: second, kind: prompt, runner: executor, depends_on: [first], prompt: "Do the next." }
"#;

const PAST_ITS_SHARE_FIXTURE: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: usage, input_tokens: 60, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 60, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 60, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
"#;

/// A person at the engine's terminal has the same answer, with the same result: no
/// interface goes past the cap by a path of its own.
#[tokio::test]
async fn a_terminal_s_continue_runs_a_cut_session_past_its_share() {
    let bench = Bench::new();
    let answered = bench
        .run_full(
            PAST_ITS_SHARE,
            PAST_ITS_SHARE_FIXTURE,
            BUDGET_CONFIG,
            &SequencedInteraction::choosing(&["continue"]),
        )
        .await;
    assert_eq!(answered.terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_recorded_continue_runs_a_cut_session_past_its_share() {
    let bench = Bench::new();
    let cut = bench
        .run_with_config(PAST_ITS_SHARE, PAST_ITS_SHARE_FIXTURE, BUDGET_CONFIG)
        .await;
    assert!(
        matches!(cut.terminal, RunTerminal::Paused { .. }),
        "{:?}",
        cut.terminal
    );

    answer_parked(&bench, "continue").await.unwrap();
    assert!(RunState::replay(&bench.events()).answer_awaits_its_resume());

    let woken = bench.wake().await;
    assert_eq!(woken.terminal, RunTerminal::Finished);
}

/// An engine that dies before it starts the attempt has spent nothing, so the answer it
/// took still stands for the next one.
#[tokio::test]
async fn an_engine_that_died_before_the_attempt_leaves_the_answer_standing() {
    let bench = Bench::new();
    bench
        .run_with_config(PAST_ITS_SHARE, PAST_ITS_SHARE_FIXTURE, BUDGET_CONFIG)
        .await;
    answer_parked(&bench, "continue").await.unwrap();
    let woke_and_died =
        EventDraft::new(bench.run_id.clone(), None, resumed()).expect("a draft names its subject");
    bench.storage.append(&woke_and_died, &FixedClock).unwrap();

    let next = bench.wake().await;
    assert_eq!(next.terminal, RunTerminal::Finished);
}

/// Two nodes under a cap of 100: the second session spends what is left of it and dies,
/// which is a failure the cap did not cause, and its next attempt has work to do.
const FAILS_AT_THE_CAP: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: usage, input_tokens: 50, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 50, output_tokens: 0 }
    outcome: { type: crash }
  - outcome: { type: completed, summary: spent }
"#;

/// How many events `kept` keeps of the log of `bench`.
fn how_many(bench: &Bench, kept: impl Fn(&EventPayload) -> bool) -> usize {
    bench
        .events()
        .iter()
        .filter(|event| event.payload().is_some_and(&kept))
        .count()
}

/// The answers and the attempts of a run stand apart: `retry` reaches the cap, and with
/// the run past it the attempt that answer asked for runs. Were the cap's stop to close
/// the answer, the engine past the cap would park on the failed node without spending,
/// and `retry` and `continue` would alternate forever.
///
/// Reached by: `crates/core/src/events/run/ledger.rs`
#[tokio::test]
async fn a_retry_that_reached_the_cap_runs_after_continue() {
    let bench = Bench::new();
    let failed = bench
        .run_with_config(PAST_ITS_SHARE, FAILS_AT_THE_CAP, BUDGET_CONFIG)
        .await;
    assert!(
        matches!(
            failed.terminal,
            RunTerminal::Paused {
                reason: PauseReason::NodeFailed { .. }
            }
        ),
        "{:?}",
        failed.terminal
    );

    answer_parked(&bench, "retry").await.unwrap();
    let on_the_cap = bench.wake().await;
    assert!(
        matches!(
            on_the_cap.terminal,
            RunTerminal::Paused {
                reason: PauseReason::BudgetExhausted { .. }
            }
        ),
        "{:?}",
        on_the_cap.terminal
    );
    let state = RunState::replay(&bench.events());
    assert!(
        !state.answer_awaits_its_resume(),
        "the run stands on the cap, and that is what is asked"
    );
    let (on, _) = current_escalation(&bench.manifest(), &bench.run_dir(), &state)
        .await
        .unwrap()
        .expect("the cap is a pause with a menu");
    assert_eq!(on, DecisionOn::Run);

    answer_parked(&bench, "continue").await.unwrap();
    let past_it = bench.wake().await;
    assert_eq!(past_it.terminal, RunTerminal::Finished);

    let answers = how_many(&bench, |payload| {
        matches!(payload, EventPayload::Gates(GateEvent::Resolved(_)))
    });
    assert_eq!(answers, 2, "two answers and the run moved on");
    let attempts = how_many(&bench, |payload| {
        matches!(payload, EventPayload::Node(NodeEvent::Started(_)))
    });
    assert_eq!(attempts, 3, "the second node ran once more");
}
