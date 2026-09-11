//! The words a decision is met by, wherever a person meets it: on the
//! menu they answer at the prompt, on the page `yunta status` prints,
//! and in the trailer that closes a parked run out.
//!
//! An escalation is one object on all three, so an option is one line on
//! all three: a reader who picked `retry` off a menu and then reads the
//! same run's page has to see the same option, not two renderings of it.

use yunta_core::events::{GateOption, GateWaitingPayload};

/// The line an option is chosen by: the id a person answers with, and
/// the label that says what choosing it does.
pub(crate) fn option_headline(option: &GateOption) -> String {
    format!("{} — {}", option.id, option.label)
}

/// What choosing an option costs, on the line under it. Every option
/// carries one, because it is what makes the choice a decision rather
/// than a guess.
pub(crate) fn option_tradeoff(option: &GateOption) -> String {
    yunta_core::text::detailed("tradeoff", &option.tradeoff)
}

/// The engine's own record of what happened, when reading it tells a
/// person something the summary did not already say. `None` when there
/// is nothing to add.
///
/// The summary is an account of what happened and the evidence is what
/// the engine derived from the log, which is why a surface puts them
/// side by side: the second is what the first is audited against. But
/// some escalations are built out of their own evidence — an exhausted
/// re-route quotes the failure that caused it into its summary — and
/// there the two are the same string. Printing it again under a heading
/// that promises the record behind the claim sends a reader hunting for
/// a difference that is not there.
pub(crate) fn evidence(escalation: &GateWaitingPayload) -> Option<&str> {
    let evidence = escalation.evidence.trim();
    if evidence.is_empty() {
        return None;
    }
    // Compared as the surfaces lay them out, not as they were stored: a
    // summary that wrapped the evidence across lines still says it.
    let summary = yunta_core::text::one_line(&escalation.summary);
    if summary.contains(&yunta_core::text::one_line(evidence)) {
        return None;
    }
    Some(evidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escalation(summary: &str, evidence: &str) -> GateWaitingPayload {
        GateWaitingPayload {
            summary: summary.to_string(),
            evidence: evidence.to_string(),
            options: Vec::new(),
            external_ref: None,
        }
    }

    #[test]
    fn an_option_reads_as_the_id_that_answers_it_and_what_it_does() {
        let option = GateOption {
            id: yunta_core::OptionId::from_static("retry"),
            label: "Re-route to `fix-lint` once more".to_string(),
            tradeoff: "Uses one extra correction attempt".to_string(),
        };
        assert_eq!(
            option_headline(&option),
            "retry — Re-route to `fix-lint` once more"
        );
        assert_eq!(
            option_tradeoff(&option),
            "tradeoff: Uses one extra correction attempt"
        );
    }

    #[test]
    fn evidence_the_summary_already_carries_is_not_offered_again() {
        // The shape an exhausted re-route has: the engine quotes the
        // cause into the summary and attaches that same cause.
        let repeated = escalation(
            "node `lint` failed and its 0 re-route(s) to `fix-lint` are exhausted: exit 1",
            "exit 1",
        );
        assert_eq!(evidence(&repeated), None);
    }

    #[test]
    fn evidence_the_summary_carries_across_a_line_break_is_not_offered_again() {
        let wrapped = escalation("node `lint` failed:\n  exit 1", "exit 1");
        assert_eq!(evidence(&wrapped), None);
    }

    #[test]
    fn evidence_that_says_more_than_the_summary_is_offered() {
        // The shape an unresolved gate has: the message it asks with
        // never names who is being asked.
        let gate = escalation("Approve the plan?", "assignee: lead");
        assert_eq!(evidence(&gate), Some("assignee: lead"));
    }

    #[test]
    fn evidence_with_nothing_in_it_is_not_offered() {
        assert_eq!(evidence(&escalation("Approve the plan?", "  \n ")), None);
    }
}
