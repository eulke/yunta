//! The decisions a scope expansion request settles without a diff, run
//! through core's `screen` and `settle`, and the batch run in which a `rules`
//! request into a path another task of the batch may write escalates to a
//! person instead of being granted.

use yunta_core::events::{Decider, EventPayload, ScopeEvent};
use yunta_core::expansion::{
    screen, settle, Ask, Bounds, Candidate, Decision, Denial, Precheck, Proposal, Screen, Waiting,
};
use yunta_core::protected::Protected;
use yunta_core::{ScopeExpansionMode, ScopeGlob};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

mod common;
use common::*;

fn bounds(mode: ScopeExpansionMode, max_per_run: Option<u32>, protected: &Protected) -> Bounds<'_> {
    Bounds {
        mode,
        max_files: 5,
        max_per_run,
        protected,
        beside: &[],
    }
}

/// The decision `screen` settles on; a panic when it leaves the request open.
fn settled(screened: Screen) -> Decision {
    match screened {
        Screen::Settled(decision) => decision,
        Screen::Candidate(_) => panic!("the request stays open for the diff"),
    }
}

/// The candidate `screen` leaves open; a panic when it settles the request.
fn open(screened: Screen) -> Candidate {
    match screened {
        Screen::Candidate(candidate) => candidate,
        Screen::Settled(decision) => panic!("the request settles as {decision:?}"),
    }
}

#[test]
fn a_proposed_criterion_that_already_passes_is_denied_without_consulting_any_mode() {
    // A proposed criterion that already passes is denied without consulting
    // anyone, even in `ask`, which would otherwise escalate.
    let protected = Protected::default();
    let paths = [ScopeGlob::from("src/x.rs")];
    let ask = Ask::Requested {
        paths: &paths,
        reason: "small adjacent fix",
        proposal: Some(Proposal {
            cmd: "true",
            precheck: Precheck::Passes,
        }),
    };
    let decision = settled(screen(
        &ask,
        &bounds(ScopeExpansionMode::Ask, None, &protected),
    ));
    assert_eq!(decision, Decision::Deny(Denial::PrecheckPasses));
}

#[test]
fn deny_mode_denies_without_running_any_rule() {
    let protected = Protected::default();
    let paths = [ScopeGlob::from("src/x.rs")];
    let ask = Ask::Requested {
        paths: &paths,
        reason: "small adjacent fix",
        proposal: None,
    };
    let decision = settled(screen(
        &ask,
        &bounds(ScopeExpansionMode::Deny, None, &protected),
    ));
    assert_eq!(decision, Decision::Deny(Denial::ModeDeny));
}

#[test]
fn ask_mode_escalates_instead_of_deciding() {
    let protected = Protected::default();
    let paths = [ScopeGlob::from("src/x.rs")];
    let ask = Ask::Requested {
        paths: &paths,
        reason: "small adjacent fix",
        proposal: Some(Proposal {
            cmd: "false",
            precheck: Precheck::Red { exit: 1 },
        }),
    };
    let decision = settled(screen(
        &ask,
        &bounds(ScopeExpansionMode::Ask, None, &protected),
    ));
    assert!(
        matches!(
            decision,
            Decision::Escalate {
                because: Waiting::ModeAsk,
                ..
            }
        ),
        "{decision:?}"
    );
}

#[test]
fn rules_mode_requires_a_proposed_criterion() {
    let protected = Protected::default();
    let paths = [ScopeGlob::from("src.rs")];
    let ask = Ask::Requested {
        paths: &paths,
        reason: "small adjacent fix",
        proposal: None,
    };
    let decision = settled(screen(
        &ask,
        &bounds(ScopeExpansionMode::Rules, None, &protected),
    ));
    assert_eq!(decision, Decision::Deny(Denial::NoCriterion));
}

#[test]
fn a_rules_request_without_a_criterion_is_denied_as_no_criterion() {
    // No ceiling is declared any more: a path is refused for want of a
    // criterion alone, wherever it points.
    let protected = Protected::default();
    let paths = [ScopeGlob::from("outside.rs")];
    let ask = Ask::Requested {
        paths: &paths,
        reason: "small adjacent fix",
        proposal: None,
    };
    let decision = settled(screen(
        &ask,
        &bounds(ScopeExpansionMode::Rules, None, &protected),
    ));
    assert_eq!(decision, Decision::Deny(Denial::NoCriterion));
}

#[test]
fn an_exhausted_cap_escalates_even_under_rules_mode() {
    let protected = Protected::default();
    let paths = [ScopeGlob::from("src.rs")];
    let ask = Ask::Requested {
        paths: &paths,
        reason: "small adjacent fix",
        proposal: Some(Proposal {
            cmd: "test -f nonexistent-marker",
            precheck: Precheck::Red { exit: 1 },
        }),
    };
    let bounds = bounds(ScopeExpansionMode::Rules, Some(2), &protected);
    let candidate = open(screen(&ask, &bounds));
    let decision = settle(candidate, 1, 2, &bounds);
    assert!(
        matches!(
            decision,
            Decision::Escalate {
                because: Waiting::CapExhausted { granted: 2, cap: 2 },
                ..
            }
        ),
        "cap exhaustion escalates rather than silently denying or granting: {decision:?}"
    );
}

#[test]
fn a_cap_not_yet_reached_does_not_escalate() {
    let protected = Protected::default();
    let paths = [ScopeGlob::from("src.rs")];
    let ask = Ask::Requested {
        paths: &paths,
        reason: "small adjacent fix",
        proposal: Some(Proposal {
            cmd: "test -f nonexistent-marker",
            precheck: Precheck::Red { exit: 1 },
        }),
    };
    let bounds = bounds(ScopeExpansionMode::Rules, Some(3), &protected);
    let candidate = open(screen(&ask, &bounds));
    let decision = settle(candidate, 1, 2, &bounds);
    assert!(
        matches!(
            decision,
            Decision::Grant {
                by: Decider::Rule,
                ..
            }
        ),
        "{decision:?}"
    );
}

#[tokio::test]
async fn a_rules_request_into_a_path_another_task_may_write_escalates() {
    let bench = Bench::new();
    let request = request_step(serde_json::json!({
        "paths": ["out-2.txt"],
        "reason": "shared file",
        "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
    }));
    let fixture = independent_tasks_fixture(2).replacen(
        "      - { path: out-1.txt, content: \"1\" }\n",
        &format!("      - {{ path: out-1.txt, content: \"1\" }}\n{request}"),
        1,
    );
    let workflow = scope_expansion_workflow("rules", None).replacen(
        "    prompt: \"Read",
        "    concurrency: 2\n    prompt: \"Read",
        1,
    );
    bench.run(&workflow, &fixture).await;
    assert!(
        bench.events().iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Scope(ScopeEvent::Requested(_)))
        )),
        "{:?}",
        bench.events()
    );
    assert!(
        !bench.events().iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Scope(ScopeEvent::Granted(_)))
        )),
        "{:?}",
        bench.events()
    );
}

/// A cap that one grant has spent leaves the next request of the run refused:
/// the second task's request has no criterion, so `rules` denies it, and the
/// run finishes rather than pausing for a person.
#[tokio::test]
async fn an_exhausted_cap_never_turns_a_refusal_into_an_escalation() {
    let bench = Bench::new();
    let granted = request_step(serde_json::json!({
        "paths": ["extra-1.txt"],
        "reason": "small adjacent fix",
        "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
    }));
    let refused = request_step(serde_json::json!({
        "paths": ["extra-2.txt"],
        "reason": "small adjacent fix",
    }));
    let fixture = independent_tasks_fixture(2)
        .replacen(
            "      - { path: out-1.txt, content: \"1\" }\n",
            &format!("      - {{ path: out-1.txt, content: \"1\" }}\n{granted}"),
            1,
        )
        .replacen(
            "      - { path: out-2.txt, content: \"2\" }\n",
            &format!("      - {{ path: out-2.txt, content: \"2\" }}\n{refused}"),
            1,
        );
    let RunReport { terminal, .. } = bench
        .run(&scope_expansion_workflow("rules", Some(1)), &fixture)
        .await;
    assert_eq!(terminal, RunTerminal::Finished, "{:?}", bench.events());
    let denied = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Denied(denied))) => Some(denied.clone()),
            _ => None,
        })
        .expect("the second request is denied");
    assert_eq!(denied.why, Some(Denial::NoCriterion));
    assert_eq!(denied.count_this_run, 1);
}

/// A person denies the request an `ask`-mode loop put to them. Both tests read
/// the one run that makes it, so its setup is written once.
mod denied_by_a_human {
    use super::*;
    use yunta_core::events::{HumanChoice, ProposedCriterion, TaskStatus};
    use yunta_engine::{RunReport, RunTerminal};
    use yunta_testkit::ScriptedInteraction;

    /// `task-n` asks for `b.txt`, which a fence would have refused; the human
    /// denies the request. The first attempt writes nothing, so its criterion
    /// stays red and the request is owed a person's decision.
    async fn run_denied() -> (Bench, RunReport) {
        let bench = Bench::new();
        let workflow = scope_expansion_workflow("ask", None);
        let tasks = format!(
            "tasks:\n{}",
            task_yaml("task-n", "n", "a.txt", "test -f a.txt")
        );
        let mut fixture = plan_session(&tasks);
        fixture.push_str(&format!(
            "  - match_prompt_contains: \"task-n\"\n{}    outcome: {{ type: completed, summary: asked }}\n",
            request_step(serde_json::json!({
                "paths": ["b.txt"],
                "reason": "adjacent fix in b.txt",
                "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
            })),
        ));
        // The fresh session the pending request opens writes nothing either.
        fixture.push_str(
            "  - match_prompt_contains: \"task-n\"\n    outcome: { type: completed, summary: fresh }\n",
        );
        // Denied, the task writes what its scope allows, and closes.
        fixture.push_str(
            "  - match_prompt_contains: \"task-n\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-n }\n",
        );
        let interaction = ScriptedInteraction::new(HumanChoice {
            option: "deny".into(),
            by: "eulke".into(),
            free_text: Some("out of this sprint".to_string()),
        });
        let report = bench
            .run_with_interaction(&workflow, &fixture, &interaction)
            .await;
        (bench, report)
    }

    /// The denial is the person's, with the choice they made, and the session's
    /// own reason and proposed criterion become the finding it leaves.
    #[tokio::test]
    async fn a_human_denial_is_recorded_as_theirs_and_leaves_a_finding() {
        let (bench, _) = run_denied().await;
        let events = bench.storage.events_for_run(&bench.run_id).unwrap();
        let denied = events
            .iter()
            .find_map(|e| match e.payload() {
                Some(EventPayload::Scope(ScopeEvent::Denied(p)))
                    if p.task_id.as_ref().is_some_and(|t| t.as_str() == "task-n") =>
                {
                    Some(p)
                }
                _ => None,
            })
            .expect("the human denial must be recorded");
        assert_eq!(denied.decided_by, Decider::Person { id: "eulke".into() });
        assert_eq!(
            denied.why(),
            Some(Denial::Chosen {
                option: "deny".to_string(),
                note: Some("out of this sprint".to_string()),
            })
        );

        // Every denial — human ones included — becomes a finding carrying the
        // agent's own reason and proposed criterion.
        let findings = findings_posted(&events);
        let finding = findings
            .iter()
            .find(|f| f.detail.contains("adjacent fix in b.txt"))
            .expect("the denial must convert into a finding");
        assert_eq!(
            finding.proposed_criterion,
            Some(ProposedCriterion {
                cmd: "test -f nonexistent-marker".to_string()
            })
        );
    }

    /// The task closes within the scope it had, and the request is on the log
    /// once: the person's refusal of it writes no second request.
    #[tokio::test]
    async fn a_task_whose_request_is_denied_closes_within_its_scope() {
        let (bench, RunReport { terminal, state }) = run_denied().await;
        assert_eq!(terminal, RunTerminal::Finished);
        assert_eq!(state.tasks.status("task-n"), Some(TaskStatus::Done));

        let events = bench.storage.events_for_run(&bench.run_id).unwrap();
        let requested = events
            .iter()
            .filter(|event| {
                matches!(
                    event.payload(),
                    Some(EventPayload::Scope(ScopeEvent::Requested(_)))
                )
            })
            .count();
        assert_eq!(requested, 1, "{events:?}");
    }
}
