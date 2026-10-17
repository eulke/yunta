//! The words a decision is met by, wherever a person meets it: on the
//! menu they answer at the prompt, on the page `yunta status` prints,
//! and in the trailer that closes a parked run out.
//!
//! An escalation is one object on all three, so an option is one line on
//! all three: a reader who picked `retry` off a menu and then reads the
//! same run's page has to see the same option, not two renderings of it.

use yunta_core::events::{
    Fact, FailureMenu, GateWaitingPayload, Menu, ProposedCheck, ScopeWait, WhyWithheld, Withheld,
};
use yunta_core::expansion::{Precheck, Waiting};
use yunta_core::shown::withheld_because;
use yunta_core::{listed_globs, text};

use crate::blocks::{departure_facts, stuck_words};

/// The facts a person reads beside an escalation's claim: what an older log
/// attached, what its menu's data says, and why each option is withheld.
pub fn facts(escalation: &GateWaitingPayload) -> Vec<Fact> {
    let mut facts = escalation.evidence().facts();
    facts.extend(menu_facts(escalation));
    facts.extend(withheld_facts(escalation));
    facts
}

/// The lines the engine's own record is shown on, one fact to a line.
/// Empty when nothing is attached.
///
/// The facts come in the order a person reads them: an older log's record
/// first, then what the escalation's menu says it is about, then why each
/// option the gate withholds is withheld. A surface with room for the claim
/// as well shows both, one beside the other, since neither repeats the other.
pub fn evidence(escalation: &GateWaitingPayload) -> Vec<String> {
    facts(escalation)
        .iter()
        .map(crate::evidence::line)
        .filter(|line| !line.is_empty())
        .collect()
}

/// The facts a menu's data gives beside its claim, in the words a person reads.
fn menu_facts(escalation: &GateWaitingPayload) -> Vec<Fact> {
    match escalation.menu() {
        Some(Menu::Gate(gate)) => vec![Fact::labelled("assignee", gate.assignee.clone())],
        Some(Menu::Published(gate)) => vec![Fact::labelled("published at", gate.url.clone())],
        Some(Menu::Budget(cap)) => vec![
            Fact::labelled("limits.max_tokens_per_run", cap.cap.to_string()),
            Fact::labelled(
                "total input+output tokens derived from the event log",
                cap.spent.to_string(),
            ),
        ],
        Some(Menu::LoopCap(cap)) => vec![
            Fact::labelled(
                "iterations already run this invocation",
                cap.ran.to_string(),
            ),
            Fact::labelled("limits.max_loop_iterations", cap.cap.to_string()),
            Fact::bare("the tasks document still has ready tasks"),
        ],
        Some(Menu::Scope(wait)) => scope_facts(wait),
        Some(Menu::Failure(menu)) => failure_facts(menu),
        Some(Menu::Departure(menu)) => departure_facts(menu),
        Some(Menu::Console(_)) | None => Vec::new(),
    }
}

/// A failure's own lines and, when it offers no remedy, the way out.
fn failure_facts(menu: &FailureMenu) -> Vec<Fact> {
    let mut facts = crate::failure::facts(&menu.failure);
    facts.extend(
        menu.stuck
            .as_ref()
            .map(|stuck| Fact::labelled("way out", stuck_words(stuck))),
    );
    facts
}

/// The facts a scope request is shown with: the paths it asks for when there
/// are any, why the loop waits, and what its pre-check found.
fn scope_facts(wait: &ScopeWait) -> Vec<Fact> {
    let mut facts = Vec::new();
    if !wait.paths.is_empty() {
        facts.push(Fact::labelled("paths", listed_globs(&wait.paths)));
    }
    facts.push(Fact::labelled("waiting", waiting(wait)));
    facts.push(Fact::labelled(
        "pre-check",
        pre_check(wait.proposed.as_ref()),
    ));
    facts
}

/// Why the loop waits on the scope request, in the words a person reads.
fn waiting(wait: &ScopeWait) -> String {
    match wait.because {
        Waiting::ModeAsk => "the loop's scope mode is `ask`, so a person grants scope".to_string(),
        Waiting::SharedWithBatch => "another task of the batch may write these paths".to_string(),
        Waiting::CapExhausted { granted, cap } => format!(
            "{granted}/{cap} {} used",
            text::agreeing(cap as usize, "grant", "grants")
        ),
        Waiting::Instructions => "an instruction file, which only a person grants".to_string(),
        Waiting::Person => "a node's own request, which a person answers".to_string(),
    }
}

/// What the criterion a session proposed found when the engine ran it.
fn pre_check(proposed: Option<&ProposedCheck>) -> String {
    match proposed {
        None => "no criterion proposed".to_string(),
        Some(ProposedCheck { cmd, precheck }) => match precheck {
            Precheck::Passes => format!("`{cmd}` passes"),
            Precheck::Red { exit } => format!("`{cmd}` exits {exit}"),
            Precheck::CouldNotRun => format!("`{cmd}` could not run"),
            Precheck::NotRun => format!("`{cmd}` was not run"),
        },
    }
}

/// Why each option is withheld, for the reasons that are data: an older log
/// wrote the reason it stored into the record it attached, so a stored
/// reason is read from that record and not written again.
fn withheld_facts(escalation: &GateWaitingPayload) -> Vec<Fact> {
    escalation
        .withheld()
        .iter()
        .filter(|withheld| matches!(withheld.because, WhyWithheld::Unprovable { .. }))
        .map(|withheld| {
            Fact::labelled(
                "withheld",
                format!("`{}` — {}", withheld.option, because(withheld)),
            )
        })
        .collect()
}

/// Why `withheld` is not offered, in the words a person reads.
pub fn because(withheld: &Withheld) -> String {
    match &withheld.because {
        WhyWithheld::Unprovable { flaws } => withheld_because(flaws.as_slice()).unwrap_or_default(),
        WhyWithheld::Logged(said) => said.as_str().to_string(),
    }
}

/// The escalation on one line, its claim and then the facts behind it,
/// for a surface with room for exactly one: the reason a run is parked
/// with, and the row a listing gives a run.
///
/// A surface with room for two parts heads each separately; a line has
/// room for neither heading. Composing them here is what keeps the page
/// and the line from disagreeing about what an escalation says.
pub fn sentence(payload: &GateWaitingPayload) -> String {
    let claim = crate::claim::sentence(payload.summary(), payload.failure());
    text::aside(claim, &evidence(payload).join("; "))
}

#[cfg(test)]
mod tests {
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
                "run `{RUN}` exhausted its token budget — limits.max_tokens_per_run: 400; total \
                 input+output tokens derived from the event log: 500"
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
            }),
        );
        assert_eq!(evidence(&gate), vec!["assignee: lead"]);
        assert_eq!(
            sentence(&gate),
            "gate `approve` needs a decision — assignee: lead"
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
                "withheld: `continue` — the plan cannot be proven as it is written: the spec's \
                 test of `T001`, `test -f a.txt`, runs none of the files the spec wrote",
            ]
        );
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
                "waiting: 1/1 grant used",
                "pre-check: `test -f a` exits 1",
            ]
        );
        assert!(
            sentence(&request).ends_with("waiting: 1/1 grant used; pre-check: `test -f a` exits 1")
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
                "wrote what no session of the run may write — a.txt: undo those changes",
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
}
