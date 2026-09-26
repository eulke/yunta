//! [`decide`] — what the run does next, as a function of the state its
//! log derives and nothing else.
//!
//! The signature already says it takes no log. What these check is the
//! consequence: two logs that derive the same state decide the same
//! thing, so an audit event nobody's state reads can never move the run.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use yunta_core::events::{
    AgentMessagePayload, AgentMessageType, BaselineCapturedPayload, BaselineOrigin,
    BaselineResults, Escalation, EventBody, EventPayload, Fact, Failure, GateEvent, GateOption,
    GateResolvedPayload, HookExecutedPayload, HookPhase, HumanChoice, NodeEvent, NodeFailedPayload,
    NodeFinishedPayload, NodeStartedPayload, RunCreatedPayload, RunEvent, SessionEvent,
    StoredEvent, TokenUsage,
};
use yunta_core::{
    CommitSha, ContentHash, DefaultOnFailure, NodeId, NonEmpty, OnInterrupt, Workflow,
};
use yunta_engine::{decide, derive, Decision, SchedulingPolicy};

const RUN: &str = "run-1";

fn at(offset_secs: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::seconds(offset_secs)
}

fn workflow() -> Workflow {
    yunta_core::yaml::parse(
        r#"
name: ship
nodes:
  - { id: plan, kind: bash, run: "true" }
  - { id: build, kind: bash, run: "true", depends_on: [plan] }
"#,
    )
    .expect("the test workflow parses")
}

fn policy() -> SchedulingPolicy {
    SchedulingPolicy {
        max_parallel_nodes: 2,
        on_interrupt: OnInterrupt::RestartNode,
        on_failure: DefaultOnFailure::Pause,
        mode_nodes: None,
        baseline_suite: None,
    }
}

fn log(entries: Vec<(Option<&str>, EventPayload)>) -> Vec<StoredEvent> {
    entries
        .into_iter()
        .enumerate()
        .map(|(index, (node, payload))| StoredEvent {
            run_id: RUN.into(),
            seq: (index as u64 + 1).into(),
            timestamp: at(index as i64),
            node_id: node.map(Into::into),
            body: EventBody::Known(payload),
        })
        .collect()
}

fn created() -> EventPayload {
    EventPayload::Run(RunEvent::Created(RunCreatedPayload {
        manifest_hash: ContentHash::sha256(b"manifest"),
        inputs: BTreeMap::new(),
        mode: "default".into(),
        promoted_from: None,
        yunta_schema: None,
        base_branch: "main".to_string(),
        base_commit: CommitSha::from("abc1234"),
    }))
}

/// An event whose kind moves no state: the run's decision must not see it.
fn audit() -> EventPayload {
    EventPayload::Session(SessionEvent::Message(AgentMessagePayload {
        message_type: AgentMessageType::Note,
        tool_name: None,
        target: None,
        input_tokens: None,
        output_tokens: None,
        cached_input_tokens: None,
        text: Some("thinking".to_string()),
    }))
}

#[test]
fn every_decision_is_a_function_of_state_alone() {
    let workflow = workflow();
    let policy = policy();

    let plain = log(vec![
        (None, created()),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "ok",
                TokenUsage::default(),
            ))),
        ),
    ]);
    // The same run, said at greater length: audit events between every
    // pair, and one more after the last. Their kinds derive nothing, so
    // both logs reach the same state.
    let chatty = log(vec![
        (None, created()),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        (Some("plan"), audit()),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::HookExecuted(HookExecutedPayload {
                phase: HookPhase::After,
                command: "cargo fmt".to_string(),
                exit_code: 0,
            })),
        ),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "ok",
                TokenUsage::default(),
            ))),
        ),
        (Some("plan"), audit()),
    ]);

    let first = decide(&workflow, &derive(&plain), &policy);
    assert_eq!(
        first,
        Decision::Execute(vec![(NodeId::from("build"), 1)]),
        "`plan` finished, so `build` is what runs next"
    );
    assert_eq!(
        decide(&workflow, &derive(&plain), &policy),
        first,
        "asking twice about one state answers twice the same"
    );
    assert_eq!(
        decide(&workflow, &derive(&chatty), &policy),
        first,
        "a longer log that derives the same state decides the same thing"
    );
}

/// The policy is an input like any other: the same state under a
/// different cap decides differently, and nothing else does.
#[test]
fn a_decision_reads_the_policy_the_run_was_frozen_with() {
    let workflow = yunta_core::yaml::parse::<Workflow>(
        r#"
name: ship
nodes:
  - { id: a, kind: bash, run: "true" }
  - { id: b, kind: bash, run: "true" }
"#,
    )
    .expect("the test workflow parses");
    let state = derive(&log(vec![(None, created())]));

    assert_eq!(
        decide(&workflow, &state, &policy()),
        Decision::Execute(vec![(NodeId::from("a"), 1), (NodeId::from("b"), 1)]),
        "two independent roots under a cap of two start together"
    );
    let narrow = SchedulingPolicy {
        max_parallel_nodes: 1,
        ..policy()
    };
    assert_eq!(
        decide(&workflow, &state, &narrow),
        Decision::Execute(vec![(NodeId::from("a"), 1)]),
        "and one at a time under a cap of one"
    );
}

/// The pair a person's answer to `plan`'s failure leaves on the log: the
/// question they were shown, and `option`.
fn chose(failure: &Failure, option: &str) -> Vec<(Option<&'static str>, EventPayload)> {
    let escalation = Escalation::new(
        "node `plan` failed",
        vec![Fact::bare(failure.to_string())].into(),
        NonEmpty::from((
            GateOption {
                id: "retry".into(),
                label: "Run `plan` again (attempt 2)".to_string(),
                tradeoff: "a fresh attempt".to_string(),
            },
            Vec::new(),
        )),
    )
    .expect("the escalation is well-formed");
    vec![
        (
            Some("plan"),
            EventPayload::Gates(GateEvent::Waiting(escalation.into_payload())),
        ),
        (
            Some("plan"),
            EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(
                HumanChoice {
                    option: option.into(),
                    by: "lead".into(),
                    free_text: None,
                },
            ))),
        ),
    ]
}

/// A failed node with no re-route, under `pause`, is a person's decision
/// — and once they chose `retry` after the failure, the decision is the
/// node's next attempt.
#[test]
fn a_failed_node_escalates_until_a_person_chooses_to_retry_it() {
    let failure = Failure::message("exit 1");
    let failed = vec![
        (None, created()),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
                failure.clone(),
                false,
                TokenUsage::default(),
            ))),
        ),
    ];
    assert_eq!(
        decide(&workflow(), &derive(&log(failed.clone())), &policy()),
        Decision::EscalateFailure {
            node: NodeId::from("plan"),
            failure: failure.clone(),
            next_attempt: 2,
        }
    );

    let mut aborted = failed.clone();
    aborted.extend(chose(&failure, "abort"));
    assert!(
        matches!(
            decide(&workflow(), &derive(&log(aborted)), &policy()),
            Decision::EscalateFailure { .. }
        ),
        "only a `retry` runs the node again"
    );

    let mut retried = failed;
    retried.extend(chose(&failure, "retry"));
    assert_eq!(
        decide(&workflow(), &derive(&log(retried)), &policy()),
        Decision::Execute(vec![(NodeId::from("plan"), 2)])
    );
}

/// The suite a run owes is a decision of the scheduler, like every other
/// thing a run does next: a pure read of what the log says the run holds.
#[test]
fn a_run_owing_a_baseline_is_told_to_measure_it_before_any_node() {
    let events = log(vec![(None, created())]);
    let policy = SchedulingPolicy {
        baseline_suite: Some("cargo test".to_string()),
        ..policy()
    };

    assert_eq!(
        decide(&workflow(), &derive(&events), &policy),
        Decision::MeasureBaseline {
            suite: "cargo test".to_string()
        },
        "a run whose config names a suite and whose log holds no measurement owes one"
    );
}

#[test]
fn a_run_born_holding_a_baseline_is_never_told_to_measure() {
    let held = EventPayload::Run(RunEvent::BaselineCaptured(BaselineCapturedPayload {
        command: "cargo test".to_string(),
        results: BaselineResults {
            exit_code: 0,
            summary: "ok".to_string(),
        },
        hash: yunta_core::sha256_hex(b"ok"),
        origin: BaselineOrigin::Inherited {
            run: "run-root".into(),
        },
    }));
    let events = log(vec![(None, created()), (None, held)]);
    let policy = SchedulingPolicy {
        baseline_suite: Some("cargo test".to_string()),
        ..policy()
    };

    assert!(
        !matches!(
            decide(&workflow(), &derive(&events), &policy),
            Decision::MeasureBaseline { .. }
        ),
        "a run born holding its lineage's measurement owes nothing"
    );
}

/// What the node waits on decides what the run does about it, never
/// what kind of node it is: a gate node whose own questions are
/// unanswered is asked again, not republished.
#[test]
fn a_node_waiting_on_questions_is_never_taken_for_a_gate() {
    let workflow: Workflow = yunta_core::yaml::parse(
        r#"
name: asking
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: "ask"
    artifacts: { produces: [questions] }
"#,
    )
    .expect("the test workflow parses");

    let asked = log(vec![
        (None, created()),
        (
            Some("ask"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        (
            Some("ask"),
            EventPayload::Gates(yunta_core::events::GateEvent::QuestionsAsked(
                yunta_core::events::QuestionsAskedPayload::new(
                    ContentHash::sha256(b"questions"),
                    vec!["scope".into()],
                    TokenUsage::default(),
                )
                .expect("a node that asked, asked something"),
            )),
        ),
    ]);

    assert_eq!(
        decide(&workflow, &derive(&asked), &policy()),
        Decision::AskQuestions {
            node: NodeId::from("ask")
        },
        "a wait on questions is a wait on questions, whatever the node is"
    );
}
