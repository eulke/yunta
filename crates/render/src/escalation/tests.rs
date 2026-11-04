use yunta_core::events::{
    AuthorGate, BudgetCap, Claim, ConsoleGate, DepartsFrom, DepartureMenu,
    DeviationDeclaredPayload, Escalation, Failure, FailureMenu, GateOption, LoopCap, Menu,
    PublishedGate, ScopeWait, Stuck,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::shown::Flaw;
use yunta_core::{NodeId, NonEmpty, RunId, TaskId};

use super::*;

/// The run the budget and the loop cap escalate for.
const RUN: &str = "01AAAAAAAAAAAAAAAAAAAAAAAA";

/// An escalation about `claim` and `menu`, which offers an abort and nothing else.
fn about(claim: Claim, menu: Menu) -> GateWaitingPayload {
    Escalation::new(
        claim,
        menu,
        NonEmpty::from((GateOption::reserved(ReservedOption::Abort), Vec::new())),
    )
    .into_payload()
}

/// The payload of a `gate_waiting` line an older log wrote.
fn older(line: serde_json::Value) -> GateWaitingPayload {
    serde_json::from_value(line).expect("an older log's escalation reads")
}

#[test]
fn a_budget_decision_shows_its_cap_and_what_the_run_spent() {
    let budget = about(
        Claim::TokensSpent {
            run: RunId::from_static(RUN),
        },
        Menu::Budget(BudgetCap {
            cap: 400,
            spent: 500,
        }),
    );
    assert_eq!(
        evidence(&budget),
        vec![
            "limits.max_tokens_per_run: 400",
            "total input+output tokens derived from the event log: 500",
        ]
    );
    assert_eq!(
        sentence(&budget),
        format!(
            "run `{RUN}` exhausted its token budget (limits.max_tokens_per_run: 400; total \
             input+output tokens derived from the event log: 500)"
        )
    );
}

#[test]
fn a_loop_cap_decision_shows_the_iterations_it_ran_and_its_cap() {
    let capped = about(
        Claim::LoopCapSpent {
            node: NodeId::from("implement"),
        },
        Menu::LoopCap(LoopCap { cap: 2, ran: 2 }),
    );
    assert_eq!(
        evidence(&capped),
        vec![
            "iterations already run this invocation: 2",
            "limits.max_loop_iterations: 2",
            "the tasks document still has ready tasks",
        ]
    );
}

#[test]
fn a_gate_shows_the_assignee_it_names() {
    let gate = about(
        Claim::NeedsDecision {
            node: NodeId::from("approve"),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".to_string(),
            node: None,
        }),
    );
    assert_eq!(evidence(&gate), vec!["assignee: lead"]);
    assert_eq!(
        sentence(&gate),
        "gate `approve` needs a decision (assignee: lead)"
    );
}

#[test]
fn a_published_gate_shows_where_its_pull_request_is() {
    let published = Escalation::published_to(
        Claim::AwaitsReview {
            node: NodeId::from("review"),
            assignee: None,
        },
        PublishedGate {
            url: "https://forge/pr/1".to_string(),
            folder: None,
        },
        "https://forge/pr/1",
    )
    .into_payload();
    assert_eq!(
        evidence(&published),
        vec!["published at: https://forge/pr/1"]
    );
}

#[test]
fn an_escalation_with_nothing_attached_shows_no_record() {
    let console = about(
        Claim::ForgeUnreachable {
            node: NodeId::from("approve"),
            assignee: "lead".to_string(),
        },
        Menu::Console(ConsoleGate {}),
    );
    assert!(evidence(&console).is_empty());
    assert!(sentence(&console).starts_with("node `approve` wants an external gate"));
}

#[test]
fn a_record_an_older_log_attached_shows_its_facts_as_they_were_stored() {
    let older_record = older(serde_json::json!({
        "summary": "Approve the plan?",
        "evidence": [{ "label": "assignee", "value": "lead" }, { "value": "exit 1" }],
        "options": [{ "id": "abort" }],
    }));
    assert_eq!(evidence(&older_record), vec!["assignee: lead", "exit 1"]);
}

#[test]
fn a_record_an_older_log_stored_as_one_string_still_shows_it() {
    let older_record = older(serde_json::json!({
        "summary": "Approve the plan?",
        "evidence": "assignee: lead",
        "options": [{ "id": "abort" }],
    }));
    assert_eq!(evidence(&older_record), vec!["assignee: lead"]);
}

#[test]
fn a_withheld_option_shows_why_it_is_withheld() {
    let flaw = Flaw::HollowSpecTest {
        task: TaskId::from("T001"),
        cmd: "test -f a.txt".to_string(),
    };
    let approve = Escalation::new(
        Claim::NeedsDecision {
            node: NodeId::from("approve"),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".to_string(),
            node: None,
        }),
        NonEmpty::from((
            GateOption::reserved(ReservedOption::Abort),
            vec![GateOption::reserved(ReservedOption::Continue)],
        )),
    )
    .withholding(vec![Withheld {
        option: "continue".into(),
        because: WhyWithheld::Unprovable {
            flaws: NonEmpty::from((flaw, Vec::new())),
        },
    }]);
    assert_eq!(
        evidence(&approve.into_payload()),
        vec![
            "assignee: lead",
            "withheld: `continue`, because the plan cannot be proven as it is written: \
             the spec's test of `T001`, `test -f a.txt`, runs none of the files the spec wrote",
        ]
    );
}

/// `noted` leaves out why an option is withheld, which `evidence` still shows.
#[test]
fn noted_leaves_out_why_an_option_is_withheld() {
    let flaw = Flaw::HollowSpecTest {
        task: TaskId::from("T001"),
        cmd: "test -f a.txt".to_string(),
    };
    let payload = Escalation::new(
        Claim::NeedsDecision {
            node: NodeId::from("approve"),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".to_string(),
            node: None,
        }),
        NonEmpty::from((
            GateOption::reserved(ReservedOption::Abort),
            vec![GateOption::reserved(ReservedOption::Continue)],
        )),
    )
    .withholding(vec![Withheld {
        option: "continue".into(),
        because: WhyWithheld::Unprovable {
            flaws: NonEmpty::from((flaw, Vec::new())),
        },
    }])
    .into_payload();
    assert_eq!(evidence(&payload).len(), 2);
    assert_eq!(noted(&payload), vec!["assignee: lead"]);
}

#[test]
fn a_withheld_reason_an_older_log_stored_is_not_written_again() {
    let stored = Withheld {
        option: "continue".into(),
        because: WhyWithheld::Logged("the plan cannot be proven as it is written".into()),
    };
    let withheld = Escalation::new(
        Claim::NeedsDecision {
            node: NodeId::from("approve"),
        },
        Menu::Console(ConsoleGate {}),
        NonEmpty::from((
            GateOption::reserved(ReservedOption::Abort),
            vec![GateOption::reserved(ReservedOption::Continue)],
        )),
    )
    .withholding(vec![stored.clone()]);
    assert!(evidence(&withheld.into_payload()).is_empty());
    assert_eq!(
        because(&stored),
        "the plan cannot be proven as it is written"
    );
}

#[test]
fn a_scope_request_shows_its_paths_why_the_loop_waits_and_its_pre_check() {
    let request = about(
        Claim::ScopeAsked {
            task: TaskId::from("task-h"),
            reason: Some("needs a.rs".to_string()),
        },
        Menu::Scope(ScopeWait {
            because: Waiting::CapExhausted { granted: 1, cap: 1 },
            paths: vec!["src/a.rs".into()],
            proposed: Some(ProposedCheck {
                cmd: "test -f a".to_string(),
                precheck: Precheck::Red { exit: 1 },
            }),
        }),
    );
    assert_eq!(
        evidence(&request),
        vec![
            "paths: src/a.rs",
            "waiting: 1 of 1 grant used",
            "pre-check: `test -f a` exits 1",
        ]
    );
    assert!(
        sentence(&request).ends_with("waiting: 1 of 1 grant used; pre-check: `test -f a` exits 1)")
    );
}

#[test]
fn a_failure_with_no_remedy_says_its_way_out_beside_what_failed() {
    let menu = FailureMenu {
        attempt: 1,
        failure: Failure::PathsDenied {
            denied_paths: vec!["a.txt".into()],
            reasons: Vec::new(),
        },
        remedies: Vec::new(),
        stuck: Some(Stuck::ProjectDenies),
        reroute: None,
    };
    let payload = about(
        Claim::NodeFailed {
            node: NodeId::from("lint"),
        },
        Menu::Failure(Box::new(menu)),
    );
    assert_eq!(
        evidence(&payload),
        vec![
            "wrote what no session of the run may write: a.txt; undo those changes",
            "way out: the project's permissions refuse it, so no attempt goes \
             differently: change what they refuse, or the permissions themselves",
        ]
    );
}

#[test]
fn a_departure_shows_what_the_session_departs_from_and_said() {
    let deviation = DeviationDeclaredPayload {
        task_id: "greet".into(),
        from: DepartsFrom::Criterion("grep -q Hello greeting.txt".to_string()),
        planned: "the greeting names the person".to_string(),
        instead: "it prints a fixed greeting".to_string(),
        why: "the person is not known yet".to_string(),
    };
    let departure = DepartureMenu {
        deviations: vec![deviation],
        waived: vec!["grep -q Hello greeting.txt".to_string()],
        rewritten_by: None,
        accept_offered: true,
    };
    let payload = about(
        Claim::Departs {
            task: TaskId::from("greet"),
        },
        Menu::Departure(Box::new(departure)),
    );
    let record = evidence(&payload);
    assert_eq!(
        record[0],
        "departs from: the criterion `grep -q Hello greeting.txt`"
    );
    assert_eq!(record[3], "because: the person is not known yet");
}
