//! A person's answer to a question about one of a loop's tasks is written on
//! the loop's own gate, and it is not the loop's decision about its failure.
//! The choice made on the failure's menu still stands after the task answers
//! that follow it, and a task answer that follows a failure with no choice
//! made on its menu is no choice about the failure at all.

use yunta_core::events::{
    Escalation, EventPayload, Evidence, Failure, FailureMenu, GateEvent, GateOption,
    GateResolvedPayload, HumanChoice, NodeEvent, NodeFailedPayload, NodeStartedPayload, ScopeWait,
    TokenUsage,
};
use yunta_core::expansion::Waiting;
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, NonEmpty, Seq};
use yunta_engine::derive;
use yunta_testkit_core::Log;

/// An option of a menu, with no words of its own.
fn option(choice: ReservedOption) -> GateOption {
    GateOption {
        id: choice.id(),
        label: None,
        tradeoff: None,
        asks: None,
    }
}

/// A menu of two options, `first` and then `second`.
fn two_options(first: ReservedOption, second: ReservedOption) -> NonEmpty<GateOption> {
    NonEmpty::from((option(first), vec![option(second)]))
}

fn started(attempt: u32) -> EventPayload {
    EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(attempt)))
}

fn failed() -> EventPayload {
    EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
        Failure::Message {
            outcome: "criteria still red".to_string(),
        },
        TokenUsage::default(),
    )))
}

/// The gate a failed loop opens: its menu offers retry, or abort.
fn failure_menu() -> EventPayload {
    let menu = FailureMenu {
        attempt: 1,
        failure: Failure::Message {
            outcome: "criteria still red".to_string(),
        },
        remedies: Vec::new(),
        stuck: None,
        reroute: None,
    };
    let escalation = Escalation::new(
        "node `loop` failed",
        Evidence::none(),
        two_options(ReservedOption::Retry, ReservedOption::Abort),
    )
    .expect("a claim with no evidence repeats none")
    .with_failure(menu);
    EventPayload::Gates(GateEvent::Waiting(escalation.into_payload()))
}

/// The gate a task's scope request opens on its loop: grant, or deny.
fn scope_request() -> EventPayload {
    let escalation = Escalation::new(
        "task `write` requests scope expansion",
        Evidence::none(),
        two_options(ReservedOption::Grant, ReservedOption::Deny),
    )
    .expect("a claim with no evidence repeats none")
    .with_scope(ScopeWait {
        because: Waiting::ModeAsk,
        proposed: None,
    });
    EventPayload::Gates(GateEvent::Waiting(escalation.into_payload()))
}

/// A person's answer to the gate written just before it.
fn answer(choice: ReservedOption) -> EventPayload {
    EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(
        HumanChoice {
            option: choice.id(),
            by: "lead".into(),
            free_text: None,
        },
    )))
}

#[test]
fn a_task_answer_after_the_retry_leaves_the_retry_as_the_choice_after_the_failure() {
    let events = Log::for_run("task-answers")
        .node("loop", started(1))
        .node("loop", failed())
        .node("loop", failure_menu())
        .node("loop", answer(ReservedOption::Retry))
        .node("loop", started(2))
        .node("loop", scope_request())
        .node("loop", answer(ReservedOption::Deny))
        .build();

    let state = derive(&events);
    assert!(state.broken.is_none(), "the log replays whole");
    let (at, choice) = state
        .choice_after_failure(&NodeId::from("loop"))
        .expect("the retry is the choice made after the failure");
    assert_eq!(at, Seq::from(4));
    assert_eq!(choice.option, ReservedOption::Retry.id());
}

#[test]
fn a_task_answer_after_a_failure_is_no_choice_about_the_failure() {
    let events = Log::for_run("task-answers")
        .node("loop", started(1))
        .node("loop", failed())
        .node("loop", scope_request())
        .node("loop", answer(ReservedOption::Deny))
        .build();

    let state = derive(&events);
    assert!(state.broken.is_none(), "the log replays whole");
    assert!(state.choice_after_failure(&NodeId::from("loop")).is_none());
}

#[test]
fn a_task_answer_after_a_failure_does_not_run_the_node_again() {
    let events = Log::for_run("task-answers")
        .node("loop", started(1))
        .node("loop", failed())
        .node("loop", scope_request())
        .node("loop", answer(ReservedOption::Grant))
        .build();

    let state = derive(&events);
    assert!(state.broken.is_none(), "the log replays whole");
    assert_eq!(state.run_again_chosen(&NodeId::from("loop")), None);
}

#[test]
fn a_task_answer_after_the_retry_leaves_the_retry_as_the_run_again_choice() {
    let events = Log::for_run("task-answers")
        .node("loop", started(1))
        .node("loop", failed())
        .node("loop", failure_menu())
        .node("loop", answer(ReservedOption::Retry))
        .node("loop", scope_request())
        .node("loop", answer(ReservedOption::Deny))
        .build();

    let state = derive(&events);
    assert!(state.broken.is_none(), "the log replays whole");
    assert_eq!(
        state.run_again_chosen(&NodeId::from("loop")),
        Some(ReservedOption::Retry)
    );
}
