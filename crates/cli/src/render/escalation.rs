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

/// The lines the engine's own record is shown on, one fact to a line.
/// Empty when nothing is attached.
///
/// The summary is an account of what happened and this is what the
/// engine read off the log, which is why a surface puts them side by
/// side: the second is what the first is audited against. Neither one
/// repeats the other, so a surface with room for both shows both.
pub(crate) fn evidence(escalation: &GateWaitingPayload) -> Vec<String> {
    escalation.evidence.lines()
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{Evidence, Fact};

    use super::*;

    fn escalation(summary: &str, evidence: Evidence) -> GateWaitingPayload {
        GateWaitingPayload {
            summary: summary.to_string(),
            evidence,
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
            "node `lint` failed and its 0 re-route(s) to `fix-lint` are exhausted",
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
}
