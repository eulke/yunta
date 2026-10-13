//! The words a decision is met by, wherever a person meets it: on the
//! menu they answer at the prompt, on the page `yunta status` prints,
//! and in the trailer that closes a parked run out.
//!
//! An escalation is one object on all three, so an option is one line on
//! all three: a reader who picked `retry` off a menu and then reads the
//! same run's page has to see the same option, not two renderings of it.

use yunta_core::events::{
    EscalationError, Fact, GateOption, GateWaitingPayload, ProposedCheck, ScopeWait,
};
use yunta_core::expansion::{Precheck, Waiting};
use yunta_core::text;

/// What an option does, when that says more than its id: an older log
/// labelled a declared option with its own id, and a menu that prints
/// both says one word twice.
pub fn label(option: &GateOption) -> Option<&str> {
    (option.label != option.id.as_str()).then_some(option.label.as_str())
}

/// The lines the engine's own record is shown on, one fact to a line.
/// Empty when nothing is attached.
///
/// The record includes why a loop waits on a scope request, and what the
/// request's proposed criterion found, after the facts attached to it.
///
/// The summary is an account of what happened and this is what the
/// engine read off the log, which is why a surface puts them side by
/// side: the second is what the first is audited against. Neither one
/// repeats the other, so a surface with room for both shows both.
pub fn evidence(escalation: &GateWaitingPayload) -> Vec<String> {
    let mut lines = crate::evidence::lines(escalation.evidence());
    if let Some(scope) = escalation.scope() {
        lines.extend(scope_facts(scope).iter().map(crate::evidence::line));
    }
    lines
}

/// The facts a scope request is shown with: why the loop waits, and what its pre-check found.
fn scope_facts(wait: &ScopeWait) -> [Fact; 2] {
    let because = match wait.because {
        Waiting::ModeAsk => "the loop's scope mode is `ask`, so a person grants scope".to_string(),
        Waiting::SharedWithBatch => "another task of the batch may write these paths".to_string(),
        Waiting::CapExhausted { granted, cap } => format!(
            "{granted}/{cap} {} used",
            text::agreeing(cap as usize, "grant", "grants")
        ),
        Waiting::Instructions => "an instruction file, which only a person grants".to_string(),
        Waiting::Person => "a node's own request, which a person answers".to_string(),
    };
    let precheck = match &wait.proposed {
        None => "no criterion proposed".to_string(),
        Some(ProposedCheck { cmd, precheck }) => match precheck {
            Precheck::Passes => format!("`{cmd}` passes"),
            Precheck::Red { exit } => format!("`{cmd}` exits {exit}"),
            Precheck::CouldNotRun => format!("`{cmd}` could not run"),
            Precheck::NotRun => format!("`{cmd}` was not run"),
        },
    };
    [
        Fact::labelled("waiting", because),
        Fact::labelled("pre-check", precheck),
    ]
}

/// The escalation on one line, its claim and then the facts behind it,
/// for a surface with room for exactly one: the reason a run is parked
/// with, and the row a listing gives a run.
///
/// A surface with room for two parts heads each separately; a line has
/// room for neither heading. Composing them here is what keeps the page
/// and the line from disagreeing about what an escalation says.
pub fn sentence(payload: &GateWaitingPayload) -> String {
    text::aside(payload.summary(), &evidence(payload).join("; "))
}

/// The fact a refused claim repeats, on the one line the record shows it
/// on. The refusal's own sentence names no fact, so a surface that names
/// the fact adds this line to it.
pub fn repeated_fact(refused: &EscalationError) -> String {
    match refused {
        EscalationError::SummaryRepeatsEvidence { fact } => crate::evidence::line(fact),
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::Escalation;
    use yunta_core::events::{Evidence, Fact};

    use super::*;

    fn escalation(summary: &str, evidence: Evidence) -> GateWaitingPayload {
        // Published to a forge: the pull request is the menu, so these
        // renderings are about the claim and the record alone.
        Escalation::published_to(summary, evidence, "https://forge/pr/1")
            .expect("the summary states no fact the evidence holds")
            .into_payload()
    }

    fn option(id: &'static str, label: &str) -> GateOption {
        GateOption {
            id: yunta_core::OptionId::from_static(id),
            label: label.to_string(),
            tradeoff: "Uses one extra correction attempt".to_string(),
            asks: None,
        }
    }

    #[test]
    fn an_option_is_labelled_only_by_what_its_id_does_not_say() {
        assert_eq!(
            label(&option("retry", "Re-route to `fix-lint` once more")),
            Some("Re-route to `fix-lint` once more")
        );
        assert_eq!(label(&option("approve", "approve")), None);
    }

    #[test]
    fn each_fact_the_engine_attached_gets_its_own_line() {
        let capped = escalation(
            "run `r` exhausted its token budget",
            vec![
                Fact::labelled("limits.max_tokens_per_run", "400"),
                Fact::labelled(
                    "total input+output tokens derived from the event log",
                    "500",
                ),
            ]
            .into(),
        );
        assert_eq!(
            evidence(&capped),
            vec![
                "limits.max_tokens_per_run: 400",
                "total input+output tokens derived from the event log: 500",
            ]
        );
    }

    #[test]
    fn a_fact_that_names_itself_is_shown_without_a_label_invented_for_it() {
        let exhausted = escalation(
            "node `lint` failed and its 0 re-routes to `fix-lint` are exhausted",
            vec![Fact::bare("exit 1")].into(),
        );
        assert_eq!(evidence(&exhausted), vec!["exit 1"]);
    }

    #[test]
    fn an_escalation_with_nothing_attached_shows_no_record() {
        assert!(evidence(&escalation("Approve the plan?", Evidence::none())).is_empty());
    }

    #[test]
    fn a_log_that_recorded_its_record_as_one_string_still_shows_it() {
        let older = escalation(
            "Approve the plan?",
            Evidence::Prose("assignee: lead".into()),
        );
        assert_eq!(evidence(&older), vec!["assignee: lead"]);
    }

    #[test]
    fn an_escalation_reads_as_its_claim_then_the_facts_behind_it() {
        let capped = escalation(
            "run `r` exhausted its token budget",
            vec![Fact::labelled("limits.max_tokens_per_run", "400")].into(),
        );
        assert_eq!(
            sentence(&capped),
            "run `r` exhausted its token budget — limits.max_tokens_per_run: 400"
        );
        assert_eq!(
            sentence(&escalation("Approve the release?", Evidence::none())),
            "Approve the release?"
        );
    }

    #[test]
    fn a_refused_claim_shows_the_fact_it_repeats_on_the_line_the_record_uses() {
        let labelled = EscalationError::SummaryRepeatsEvidence {
            fact: Fact::labelled("limits.max_tokens_per_run", "400"),
        };
        assert_eq!(repeated_fact(&labelled), "limits.max_tokens_per_run: 400");
        let bare = EscalationError::SummaryRepeatsEvidence {
            fact: Fact::bare("exit 1"),
        };
        assert_eq!(repeated_fact(&bare), "exit 1");
    }

    #[test]
    fn a_scope_gate_says_why_the_loop_waits_and_what_its_pre_check_found() {
        let payload = Escalation::published_to(
            "task `t` requests scope expansion: needs a.rs",
            vec![Fact::labelled("paths", "src/a.rs")].into(),
            "https://forge/pr/1",
        )
        .expect("the summary states no fact the evidence holds")
        .with_scope(ScopeWait {
            because: Waiting::CapExhausted { granted: 1, cap: 1 },
            proposed: Some(ProposedCheck {
                cmd: "test -f a".to_string(),
                precheck: Precheck::Red { exit: 1 },
            }),
        })
        .into_payload();
        assert_eq!(
            evidence(&payload),
            vec![
                "paths: src/a.rs",
                "waiting: 1/1 grant used",
                "pre-check: `test -f a` exits 1",
            ]
        );
        assert!(
            sentence(&payload).ends_with("waiting: 1/1 grant used; pre-check: `test -f a` exits 1")
        );
    }
}
