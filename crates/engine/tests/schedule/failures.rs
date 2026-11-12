//! What a failure decides: a failed node is a person's decision, its menu
//! offers only what can help, and a person's choice runs the node again
//! only where the menu offered it.

use super::*;

/// The pair a person's answer to `plan`'s failure leaves on the log: the
/// question they were shown, and `option`.
fn chose(option: &str) -> Vec<(Option<&'static str>, EventPayload)> {
    let escalation = Escalation::new(
        yunta_core::events::Claim::Written("node `plan` failed".to_string()),
        yunta_core::events::Menu::Failure(Box::new(yunta_core::events::FailureMenu {
            attempt: 2,
            failure: Failure::Message {
                outcome: "exit 1".into(),
            },
            remedies: vec![Remedy {
                option: ReservedOption::Retry,
                precondition: Some(Precondition::OutsideTreeChanged),
            }],
            stuck: None,
            reroute: None,
        })),
        NonEmpty::from((GateOption::reserved(ReservedOption::Retry), Vec::new())),
    );
    vec![
        (
            Some("plan"),
            EventPayload::Gates(GateEvent::Waiting(Box::new(escalation.into_payload()))),
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

/// A failed node with no reroute, under `pause`, is a person's decision
/// — and once they chose `retry` after the failure, the decision is the
/// node's next attempt.
#[test]
fn a_failed_node_escalates_until_a_person_chooses_to_retry_it() {
    let outcome = "exit 1".into();
    let failure = Failure::Message { outcome };
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
                TokenUsage::default(),
            ))),
        ),
    ];
    assert_eq!(
        decide(
            &workflow(),
            &RunState::replay(&log(failed.clone())),
            &policy()
        ),
        Decision::EscalateFailure {
            node: NodeId::from("plan"),
            failure: failure.clone(),
            next_attempt: 2,
            outlook: Outlook::Choose(vec![Remedy {
                option: ReservedOption::Retry,
                precondition: Some(Precondition::OutsideTreeChanged),
            }]),
        }
    );

    let mut aborted = failed.clone();
    aborted.extend(chose("abort"));
    assert!(
        matches!(
            decide(&workflow(), &RunState::replay(&log(aborted)), &policy()),
            Decision::EscalateFailure { .. }
        ),
        "only a `retry` runs the node again"
    );

    let mut retried = failed;
    retried.extend(chose("retry"));
    assert_eq!(
        decide(&workflow(), &RunState::replay(&log(retried)), &policy()),
        Decision::Execute(vec![(NodeId::from("plan"), 2)])
    );
}

/// A `grant` is a choice to run the node again, its scope widened first
/// by the attempt that starts.
#[test]
fn a_grant_after_a_scope_failure_runs_the_node_again() {
    let failure = Failure::scope_violated(vec!["Cargo.toml".into()]);
    let mut events = vec![
        (None, created()),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
                failure.clone(),
                TokenUsage::default(),
            ))),
        ),
    ];
    events.extend(chose("grant"));
    assert_eq!(
        decide(&workflow(), &RunState::replay(&log(events)), &policy()),
        Decision::Execute(vec![(NodeId::from("plan"), 2)])
    );
}

/// Whether the menu of `node`'s failure offers `grant`, for a workflow
/// that declares one scoped node and one read-only node.
fn grantable(node: &'static str, failure: Failure, policy: &SchedulingPolicy) -> Decision {
    let workflow = yunta_core::yaml::parse(
        r#"
name: scoped
nodes:
  - { id: fix, kind: bash, run: "true", scope: ["src/**"] }
  - { id: audit, kind: prompt, runner: planner, prompt: "look", permissions: read-only }
"#,
    )
    .expect("the test workflow parses");
    let events = log(vec![
        (None, created()),
        (
            Some(node),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        (
            Some(node),
            EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
                failure,
                TokenUsage::default(),
            ))),
        ),
    ]);
    decide(&workflow, &RunState::replay(&events), policy)
}

/// A node that failed on its scope may have it widened by a person —
/// but not a read-only node, whose word is its whole ceiling, and not
/// under a permission ceiling that denies every expansion.
#[test]
fn a_scope_failure_offers_a_grant_only_where_a_person_may_widen_it() {
    let outside = || Failure::scope_violated(vec!["Cargo.toml".into()]);
    let offers = |decision: Decision| match decision {
        Decision::EscalateFailure {
            outlook: Outlook::Choose(remedies),
            ..
        } => remedies.iter().any(|r| r.option == ReservedOption::Grant),
        Decision::EscalateFailure { .. } => false,
        other => panic!("a failed node is a decision, got {other:?}"),
    };

    let outcome = "exit 1".into();
    assert!(offers(grantable("fix", outside(), &policy())));
    assert!(
        !offers(grantable("fix", Failure::Message { outcome }, &policy())),
        "a wider scope changes nothing about a failure that is not about scope"
    );
    assert!(
        !offers(grantable("audit", outside(), &policy())),
        "a read-only node is never widened"
    );
    let denying = SchedulingPolicy {
        grants_scope: false,
        ..policy()
    };
    assert!(!offers(grantable("fix", outside(), &denying)));
}

/// A grant that would widen a node over what the project denies to every
/// run is never on its menu, whatever else the failure is about.
#[test]
fn grant_is_not_offered_for_a_denied_path() {
    let offers = |decision: Decision| match decision {
        Decision::EscalateFailure {
            outlook: Outlook::Choose(remedies),
            ..
        } => remedies.iter().any(|r| r.option == ReservedOption::Grant),
        Decision::EscalateFailure { .. } => false,
        other => panic!("a failed node is a decision, got {other:?}"),
    };
    let protected =
        Protected::new(&[ScopeGlob::from(".github/**")], &[]).expect("the one glob builds");
    let denying_ci = SchedulingPolicy {
        protected: Arc::new(protected),
        follows_builds: false,
        ..policy()
    };
    let into_ci = Failure::scope_violated(vec![".github/ci.yml".into()]);

    assert!(offers(grantable("fix", into_ci.clone(), &policy())));
    assert!(!offers(grantable("fix", into_ci, &denying_ci)));
    assert!(offers(grantable(
        "fix",
        Failure::scope_violated(vec!["Cargo.toml".into()]),
        &denying_ci
    )));
}

/// On a red check's tree a retry is offered only once a wake finds the tree
/// moved: until then the menu offers nothing, and a person's choice to retry
/// is asked again rather than run.
#[test]
fn a_retry_on_a_red_check_runs_the_node_once_a_wake_finds_the_tree_moved() {
    let left: TreeId = "a1a1a1a".parse().expect("a tree id");
    let red = Failure::Red {
        red: RedCheck::Command {
            cmd: "cargo test".to_string(),
            exit_code: 101,
            conclusion: None,
            output: None,
        },
    };
    let started = NodeStartedPayload::attempt_from(1, left.clone());
    let failed = NodeFailedPayload::new(red.clone(), TokenUsage::default()).leaving(left);
    let mut events = vec![
        (None, created()),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Started(started)),
        ),
        (Some("plan"), EventPayload::Node(NodeEvent::Failed(failed))),
    ];
    events.extend(chose("retry"));
    let asked_again = Decision::EscalateFailure {
        node: NodeId::from("plan"),
        failure: red,
        next_attempt: 2,
        outlook: Outlook::Stuck(Stuck::TreeUnchanged),
    };
    assert_eq!(
        decide(
            &workflow(),
            &RunState::replay(&log(events.clone())),
            &policy()
        ),
        asked_again
    );

    let later: TreeId = "b2b2b2b".parse().expect("a tree id");
    let wake = RunResumedPayload::new(Vec::new(), None).finding(later);
    events.push((None, EventPayload::Run(RunEvent::Resumed(wake))));
    assert_eq!(
        decide(&workflow(), &RunState::replay(&log(events)), &policy()),
        Decision::Execute(vec![(NodeId::from("plan"), 2)]),
    );
}
