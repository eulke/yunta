//! The words of every option a decision offers: each reads as the engine
//! wrote it from its data and its menu, and the words an older log stored
//! with an option read as it wrote them.

use yunta_core::events::{
    Asks, AuthorGate, BudgetCap, Claim, ConsoleGate, DepartsFrom, DepartureMenu,
    DeviationDeclaredPayload, Escalation, GateOption, Leads, LoopCap, Menu, ScopeWait,
};
use yunta_core::expansion::Waiting;
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, NonEmpty, OptionId, RunId, TaskId};

use crate::offers::{asks, option_words};

/// An escalation about `claim` and `menu`, offering `options` in order.
fn menu(claim: Claim, menu: Menu, options: Vec<GateOption>) -> Escalation {
    let mut options = options.into_iter();
    let first = options.next().expect("a menu offers an option");
    Escalation::new(claim, menu, NonEmpty::from((first, options.collect())))
}

/// The label and the trade-off of the option `id` on `escalation`.
fn words(escalation: &Escalation, id: &str) -> (String, Option<String>) {
    let payload = escalation.clone().into_payload();
    let option = payload
        .options()
        .iter()
        .find(|option| option.id.as_str() == id)
        .expect("on the menu")
        .clone();
    option_words(&payload, &option)
}

fn said(label: &str, tradeoff: &str) -> (String, Option<String>) {
    (label.to_string(), Some(tradeoff.to_string()))
}

/// The option an author's gate offers beside its abort, where `option` leads.
fn gate(option: GateOption) -> Escalation {
    menu(
        Claim::NeedsDecision {
            node: "gate".into(),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".into(),
        }),
        vec![option, GateOption::reserved(ReservedOption::Abort)],
    )
}

/// An author's option that sends the run back to `plan`.
fn back(resumes: bool) -> GateOption {
    GateOption::leading(
        OptionId::from_static("revise"),
        Leads::Back {
            node: "plan".into(),
            resumes,
        },
    )
}

/// An author's option that takes the run on past the gate, to `next`.
fn on(next: Vec<NodeId>, settles: bool) -> GateOption {
    GateOption::leading(
        OptionId::from_static("proceed"),
        Leads::On { next, settles },
    )
}

/// The options of a spent token cap.
fn token_cap_reads_as_written() {
    let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let tokens = menu(
        Claim::TokensSpent { run },
        Menu::Budget(BudgetCap {
            cap: 100,
            spent: 200,
        }),
        vec![
            GateOption::reserved(ReservedOption::Continue),
            GateOption::reserved(ReservedOption::Abort),
        ],
    );
    assert_eq!(
        words(&tokens, "continue"),
        said(
            "Continue past the cap",
            "Lifts the cap for this invocation only; a later resume will ask again before \
             spending more"
        )
    );
    assert_eq!(
        words(&tokens, "abort"),
        said(
            "Pause the run",
            "The run pauses with reason `budget`; a resume re-asks"
        )
    );
}

/// The options of a spent loop cap.
fn loop_cap_reads_as_written() {
    let iterations = menu(
        Claim::LoopCapSpent {
            node: "implement".into(),
        },
        Menu::LoopCap(LoopCap { cap: 2, ran: 2 }),
        vec![
            GateOption::reserved(ReservedOption::Continue),
            GateOption::reserved(ReservedOption::Abort),
        ],
    );
    assert_eq!(
        words(&iterations, "continue"),
        said(
            "Keep iterating",
            "Lifts the cap for this invocation only; a later resume will ask again"
        )
    );
    assert_eq!(
        words(&iterations, "abort"),
        said(
            "Fail the loop node",
            "The node fails naming the limit and the run pauses; a resume re-runs the loop and \
             re-asks"
        )
    );
}

/// The options a console answers with, when no forge does.
fn console_reads_as_written() {
    let console = menu(
        Claim::ForgeUnreachable {
            node: "gate".into(),
            assignee: "lead".into(),
        },
        Menu::Console(ConsoleGate {}),
        vec![
            GateOption::reserved(ReservedOption::Approve),
            GateOption::reserved(ReservedOption::Reject),
        ],
    );
    assert_eq!(
        words(&console, "approve"),
        said("Approve", "Marks the gate as passed; the run continues")
    );
    assert_eq!(
        words(&console, "reject"),
        said(
            "Reject",
            "Fails the node; its declared re-route (if any) takes over"
        )
    );
}

/// The options a scope request puts to a person.
fn scope_reads_as_written() {
    let scope = menu(
        Claim::ScopeAsked {
            task: "t".into(),
            reason: None,
        },
        Menu::Scope(ScopeWait {
            because: Waiting::ModeAsk,
            paths: vec!["b.txt".into()],
            proposed: None,
        }),
        vec![
            GateOption::reserved(ReservedOption::Grant),
            GateOption::reserved(ReservedOption::Deny),
        ],
    );
    assert_eq!(
        words(&scope, "grant"),
        said(
            "Grant access to b.txt",
            "The task's final diff is evaluated against its scope plus these paths"
        )
    );
    // Only the engine's own grants count toward `max_per_run`, so a grant a
    // person picks from this menu does not say it consumes the cap.
    let (_, grant_tradeoff) = words(&scope, "grant");
    assert!(
        !grant_tradeoff.unwrap_or_default().contains("max_per_run"),
        "a person's grant does not name the run's cap"
    );
    assert_eq!(
        words(&scope, "deny"),
        said(
            "Deny the request",
            "The denial becomes a finding; the task retries within its original scope"
        )
    );
}

/// The options of an author's gate that send the run back.
fn back_options_read_as_written() {
    assert_eq!(
        words(&gate(back(true)), "revise"),
        said(
            "Back to `plan`",
            "sends what you say back to `plan`, and asks again once it completes"
        )
    );
    assert_eq!(
        words(&gate(back(false)), "revise"),
        said(
            "Back to `plan`",
            "re-routes to `plan` and asks again once it completes"
        )
    );
}

/// The options of an author's gate that take the run on past it.
fn on_options_read_as_written() {
    assert_eq!(
        words(&gate(on(Vec::new(), false)), "proceed"),
        said(
            "Resolve the gate",
            "resolves this gate; nothing in the workflow waits on it"
        )
    );
    assert_eq!(
        words(&gate(on(Vec::new(), true)), "proceed"),
        said(
            "Resolve the gate",
            "resolves this gate; nothing in the workflow waits on it, and settles every finding \
             it shows"
        )
    );
    assert_eq!(
        words(&gate(on(vec!["pr".into()], true)), "proceed"),
        said(
            "On to `pr`",
            "resolves this gate; `pr` runs next, and settles every finding it shows"
        )
    );
    assert_eq!(
        words(&gate(on(vec!["pr".into()], false)), "abort"),
        said("Abort the run", "Pauses here; nothing further executes")
    );
}

/// The questions the options that ask for words put to a person.
fn asks_read_as_written() {
    assert_eq!(asks(&back(true)).as_deref(), Some("what should change?"));
    assert_eq!(back(false).asks(), None);
    assert_eq!(
        GateOption::reserved(ReservedOption::SendBack).asks(),
        Some(Asks::Instead)
    );
}

#[test]
fn every_option_reads_as_the_engine_wrote_it() {
    token_cap_reads_as_written();
    loop_cap_reads_as_written();
    console_reads_as_written();
    scope_reads_as_written();
    back_options_read_as_written();
    on_options_read_as_written();
    asks_read_as_written();
}

#[test]
fn an_option_an_older_log_wrote_reads_as_the_words_it_stored() {
    let stored: GateOption = serde_json::from_value(serde_json::json!({
        "id": "revise",
        "label": "Revise the plan",
        "tradeoff": "Sends the plan back to its author",
        "asks": "what should the author change?",
    }))
    .expect("an older option reads");
    let payload = menu(
        Claim::NeedsDecision {
            node: "approve".into(),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".into(),
        }),
        vec![stored.clone()],
    )
    .into_payload();
    assert_eq!(
        option_words(&payload, &stored),
        (
            "Revise the plan".to_string(),
            Some("Sends the plan back to its author".to_string())
        )
    );
    assert_eq!(
        asks(&stored).as_deref(),
        Some("what should the author change?")
    );
}

#[test]
fn an_option_no_menu_words_keeps_its_id_as_its_label() {
    let retry_gate = menu(
        Claim::NeedsDecision {
            node: "approve".into(),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".into(),
        }),
        vec![
            GateOption::reserved(ReservedOption::Retry),
            GateOption::reserved(ReservedOption::Abort),
        ],
    );
    assert_eq!(words(&retry_gate, "retry"), ("retry".to_string(), None));
}

#[test]
fn a_departure_offers_accepting_it_and_sending_it_back() {
    let deviation = DeviationDeclaredPayload {
        task_id: "greet".into(),
        from: DepartsFrom::Criterion("grep -q Hello greeting.txt".to_string()),
        planned: "the greeting names the person".to_string(),
        instead: "it prints a fixed greeting".to_string(),
        why: "the person is not known yet".to_string(),
    };
    let departure = menu(
        Claim::Departs {
            task: TaskId::from("greet"),
        },
        Menu::Departure(Box::new(DepartureMenu {
            deviations: vec![deviation],
            waived: vec!["grep -q Hello greeting.txt".to_string()],
            rewritten_by: None,
            accept_offered: true,
        })),
        vec![
            GateOption::reserved(ReservedOption::Accept),
            GateOption::reserved(ReservedOption::SendBack),
        ],
    );
    let (label, tradeoff) = words(&departure, "accept");
    assert_eq!(label, "Accept the departure");
    assert!(
        tradeoff.as_deref().is_some_and(|tradeoff| tradeoff
            .starts_with("The task stops being held to `grep -q Hello greeting.txt` and closes")),
        "{}",
        tradeoff.as_deref().unwrap_or("none")
    );
    assert_eq!(
        words(&departure, "send-back"),
        said(
            "Send it back",
            "Its session picks the work back up with what you say"
        )
    );
}
