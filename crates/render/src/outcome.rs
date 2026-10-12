//! The words a node's outcome is said in, the drift in the environment a
//! run's commands run with, and the reason a run promoted.
//!
//! A node closes with either the agent's own summary or a fact the engine
//! checked. The fact is what the log holds; its sentence is produced here,
//! once, for every surface that shows how a node ended.

use yunta_core::events::{
    Approval, EngineFact, EnvironmentDrift, Evidence, NodeOutcome, PromotionCause,
};
use yunta_core::text;

use crate::evidence;

/// What a node closed with, as a reader meets it: the agent's own summary
/// as it was written, or the engine's check of the node in its own words.
pub fn sentence(outcome: &NodeOutcome) -> String {
    match outcome {
        NodeOutcome::Summary(summary) => summary.clone(),
        NodeOutcome::Fact(fact) => fact_sentence(fact),
    }
}

/// An engine fact in the words its check reports it in.
fn fact_sentence(fact: &EngineFact) -> String {
    match fact {
        EngineFact::ExitStatus { code } => format!("exit {code}"),
        EngineFact::PullRequestOpened { number, url } => {
            format!("pull request #{number} — {url}")
        }
        EngineFact::ChildrenFinished { count } => {
            format!("{} finished", text::counted_as(*count, "child", "children"))
        }
        EngineFact::SucceededFirst { node } => format!("`{node}` succeeded first"),
        EngineFact::TasksDone { count } => format!("{} done", text::counted(*count, "task")),
        EngineFact::ChildRunFinished { child } => format!("child run `{child}` finished"),
        EngineFact::NothingToCompare { command, exit_code } => format!(
            "nothing to compare: `{command}` was already red when the lineage measured it \
             (exit {exit_code})"
        ),
        EngineFact::NoRegression { exit_code, reused } => no_regression(*exit_code, *reused),
        EngineFact::CoverageMet {
            measured,
            threshold,
        } => format!("coverage {measured}% meets the {threshold}% threshold"),
        EngineFact::NoFinding { severity } => {
            format!("no finding at or above {}", severity.as_str())
        }
        EngineFact::QuestionsAnswered => "questions answered".to_string(),
        EngineFact::QuestionsAssumed => "questions assumed".to_string(),
        EngineFact::OptionChosen { option } => option.to_string(),
        EngineFact::Approved { by, on } => match on {
            Approval::Console => format!("approved from the console by {by}"),
            Approval::Review => format!("approved by {by}"),
            Approval::Merge => format!("merged by {by}"),
        },
    }
}

/// A comparison that found no regression. The memo does not say who asked
/// first, so the words say only whether the answer was already in hand.
fn no_regression(exit_code: i32, reused: bool) -> String {
    match reused {
        false => format!("no regression vs baseline (exit {exit_code})"),
        true => format!(
            "no regression vs baseline (exit {exit_code}, reused: the suite already ran on this \
             same tree)"
        ),
    }
}

/// How the environment a run's commands run with changed between the
/// run's birth and its latest wake, one clause for each part that moved.
pub fn environment_drift(drift: &EnvironmentDrift) -> String {
    let mut said: Vec<String> = Vec::new();
    if let Some((before, after)) = &drift.shell {
        said.push(format!("the shell went from {before} to {after}"));
    }
    if !drift.gained.is_empty() {
        said.push(format!("PATH gained {}", drift.gained.join(", ")));
    }
    if !drift.lost.is_empty() {
        said.push(format!("PATH lost {}", drift.lost.join(", ")));
    }
    if drift.reordered {
        said.push("PATH looks its directories up in another order".to_string());
    }
    format!(
        "commands now run in another environment than the run was born in: {}",
        said.join("; ")
    )
}

/// Why a run promoted, on one line: the node that exhausted its re-routes
/// and the record behind it. A log that stored a sentence shows it as
/// written, and a log that stored neither shows the record it has.
pub fn promotion_reason(
    cause: Option<&PromotionCause>,
    legacy: Option<&str>,
    evidence: &Evidence,
) -> String {
    match (cause, legacy) {
        (Some(promoted), _) => text::aside(
            format!(
                "node `{}` exhausted its re-routes to `{}`",
                promoted.node, promoted.goto
            ),
            &evidence::one_line(evidence),
        ),
        (None, Some(sentence)) => sentence.to_string(),
        (None, None) => evidence::one_line(evidence),
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::Fact;
    use yunta_core::NodeId;

    use super::*;

    #[test]
    fn a_summary_reads_as_the_agent_wrote_it() {
        let summary = NodeOutcome::Summary("reviewed the diff: no issues".to_string());
        assert_eq!(sentence(&summary), "reviewed the diff: no issues");
    }

    #[test]
    fn an_engine_fact_reads_in_the_words_its_check_uses() {
        let exit = NodeOutcome::Fact(EngineFact::ExitStatus { code: 0 });
        assert_eq!(sentence(&exit), "exit 0");
        let done = NodeOutcome::Fact(EngineFact::TasksDone { count: 1 });
        assert_eq!(sentence(&done), "1 task done");
        let reused = NodeOutcome::Fact(EngineFact::NoRegression {
            exit_code: 0,
            reused: true,
        });
        assert_eq!(
            sentence(&reused),
            "no regression vs baseline (exit 0, reused: the suite already ran on this same tree)"
        );
    }

    #[test]
    fn a_person_s_decision_reads_as_what_they_did() {
        let said = |fact| sentence(&NodeOutcome::Fact(fact));
        assert_eq!(said(EngineFact::QuestionsAnswered), "questions answered");
        assert_eq!(said(EngineFact::QuestionsAssumed), "questions assumed");
        let option = yunta_core::OptionId::from_static("ship");
        assert_eq!(said(EngineFact::OptionChosen { option }), "ship");
        let by = yunta_core::Responder::from_static("ana");
        for (on, words) in [
            (Approval::Console, "approved from the console by ana"),
            (Approval::Review, "approved by ana"),
            (Approval::Merge, "merged by ana"),
        ] {
            let by = by.clone();
            assert_eq!(said(EngineFact::Approved { by, on }), words);
        }
    }

    #[test]
    fn a_drift_that_gained_and_lost_path_entries_names_both() {
        let drift = EnvironmentDrift {
            shell: None,
            gained: vec!["/opt/tools".to_string()],
            lost: vec!["/opt/old".to_string()],
            reordered: false,
        };
        assert_eq!(
            environment_drift(&drift),
            "commands now run in another environment than the run was born in: PATH gained \
             /opt/tools; PATH lost /opt/old"
        );
    }

    #[test]
    fn a_drift_names_each_part_of_the_environment_that_moved() {
        let drift = EnvironmentDrift {
            shell: Some(("/bin/sh".to_string(), "/bin/zsh".to_string())),
            gained: vec!["/opt/bin".to_string()],
            lost: Vec::new(),
            reordered: false,
        };
        assert_eq!(
            environment_drift(&drift),
            "commands now run in another environment than the run was born in: the shell went \
             from /bin/sh to /bin/zsh; PATH gained /opt/bin"
        );
    }

    #[test]
    fn a_promotion_names_its_node_and_the_record_behind_it() {
        let cause = PromotionCause {
            node: NodeId::from("lint"),
            goto: NodeId::from("fix-lint"),
        };
        let record = Evidence::from(vec![Fact::bare("exit 1")]);
        assert_eq!(
            promotion_reason(Some(&cause), None, &record),
            "node `lint` exhausted its re-routes to `fix-lint` — exit 1"
        );
        assert_eq!(
            promotion_reason(None, Some("an older sentence"), &record),
            "an older sentence"
        );
    }
}
