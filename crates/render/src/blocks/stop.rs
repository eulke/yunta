//! Why a task stopped, in the words a person reads: what the pre-check
//! found wrong, how each attempt ended, and what stopped the cycle. A
//! `blocked` status carries the cause as data, so a resume reads the same
//! words the run first showed.

use std::path::PathBuf;

use yunta_core::events::{
    CriterionResult, DeviationDeclaredPayload, Surprise, TaskCause, Unanswered,
};
use yunta_core::{text, NonEmpty};

use crate::failure;

/// The sentence a task's stop reads as: every line a person needs to see
/// why the task is not done.
pub fn sentence(cause: &TaskCause) -> String {
    match cause {
        // One line per surprise: a task whose criteria are wrong in two
        // ways is wrong in two ways, and a reader fixes both.
        TaskCause::PreCheck { found } => found
            .as_slice()
            .iter()
            .map(surprise)
            .collect::<Vec<_>>()
            .join("\n"),
        TaskCause::Unmet {
            attempts,
            red,
            outside,
        } => unmet(*attempts, red, outside),
        TaskCause::Unrunnable { runs } => format!(
            "a criterion could not run, and another attempt would not change that: {}",
            exits(runs, "exits")
        ),
        TaskCause::SessionRefused(refusal) => failure::refusal_sentence(refusal),
        TaskCause::SessionLimited(limit) => failure::limit_sentence(limit),
        TaskCause::SessionDied(died) => failure::session_death(died),
        TaskCause::CarriedWorkNoLongerApplies { paths } => format!(
            "the work its last attempt left no longer applies on the run's tree (conflicts in \
             {}); run it again from scratch once the run's tree changes",
            failure::paths_listed(paths)
        ),
        TaskCause::CommandDenied { rule } => rule.clone(),
        TaskCause::ScopeDecisionOwed => {
            "a scope expansion request needs a human decision".to_string()
        }
        TaskCause::DeviationOwed { deviations } => departed(deviations),
        TaskCause::CriterionUnpassable { declared } => format!(
            "criterion `{}` cannot pass on the work of this plan: {}",
            declared.criterion, declared.why
        ),
    }
}

/// What a criterion's first run told the pre-check, in a sentence of its own.
fn surprise(surprise: &Surprise) -> String {
    match surprise {
        Surprise::TrivialCriterion { cmd } => format!(
            "criterion `{cmd}` already passes before any work — the criteria need fixing, not \
             the task"
        ),
        Surprise::BrokenGuard {
            cmd,
            proves: Some(proves),
        } => format!(
            "guard `{cmd}` is already red before any work started — it is there to show that \
             {proves}"
        ),
        Surprise::BrokenGuard { cmd, proves: None } => {
            format!("guard `{cmd}` is already red before any work started")
        }
        Surprise::Unrunnable { cmd, exit_code } => format!(
            "criterion `{cmd}` could not run: exit {} — the criteria need fixing, or the \
             environment the engine runs them in does",
            exit_described(*exit_code)
        ),
    }
}

/// An exit code as a person reads it: the code, and what it means when the
/// command never answered.
fn exit_described(exit_code: i32) -> String {
    match Unanswered::of(exit_code) {
        Some(why) => format!("{exit_code} ({})", unanswered_words(why)),
        None => exit_code.to_string(),
    }
}

/// What a command that never answered is called, in a person's words.
fn unanswered_words(why: Unanswered) -> &'static str {
    match why {
        Unanswered::NotFound => "command not found",
        Unanswered::NotExecutable => "command not executable",
        Unanswered::Stopped => "stopped before it answered",
    }
}

/// Each run as "`cmd` <verb> <exit code and what it means>", and the line
/// it concluded with, in order: a person deciding about the task reads why
/// it fails without opening what the run kept of it.
fn exits(runs: &[CriterionResult], verb: &str) -> String {
    runs.iter()
        .map(|run| {
            text::aside(
                format!("`{}` {verb} {}", run.cmd, exit_described(run.exit_code)),
                run.conclusion.as_deref().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// What an exhausted task's last attempt left: the criteria still red, and
/// the paths it changed outside the scope.
fn unmet(attempts: u32, red: &[CriterionResult], outside: &[PathBuf]) -> String {
    let said = format!(
        "not done after {}",
        text::counted(attempts as usize, "attempt")
    );
    match (red.is_empty(), outside.is_empty()) {
        (false, false) => format!(
            "{said}: {}, and the work changed {} outside its scope",
            exits(red, "still exits"),
            failure::paths_listed(outside)
        ),
        (false, true) => format!("{said}: {}", exits(red, "still exits")),
        (true, false) => format!(
            "{said}: the criteria pass, but the work changed {} outside its scope",
            failure::paths_listed(outside)
        ),
        (true, true) => said,
    }
}

/// One line per departure, each in the session's own words.
fn departed(deviations: &NonEmpty<DeviationDeclaredPayload>) -> String {
    deviations
        .as_slice()
        .iter()
        .map(|d| {
            format!(
                "its session departs from the plan: {}: {} — {}",
                d.from, d.instead, d.why
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::events::CriterionResult;

    #[test]
    fn an_unrunnable_criterion_says_how_it_exited() {
        let cause = TaskCause::Unrunnable {
            runs: vec![CriterionResult {
                cmd: "yunta-no-such-tool".to_string(),
                exit_code: 127,
                r#type: None,
                reused: false,
                duration_ms: None,
                output: None,
                tail: Vec::new(),
                tree: None,
                head: None,
                conclusion: None,
            }],
        };
        let said = sentence(&cause);
        assert!(said.contains("exits 127 (command not found)"), "{said}");
    }

    fn run(cmd: &str, exit_code: i32) -> CriterionResult {
        CriterionResult {
            cmd: cmd.to_string(),
            exit_code,
            r#type: None,
            reused: false,
            duration_ms: None,
            output: None,
            tail: Vec::new(),
            tree: None,
            head: None,
            conclusion: Some("3 failed".to_string()),
        }
    }

    #[test]
    fn a_red_criterion_says_its_exit_and_the_line_it_concluded_with() {
        let cause = TaskCause::Unmet {
            attempts: 2,
            red: vec![run("cargo test", 101)],
            outside: Vec::new(),
        };
        assert_eq!(
            sentence(&cause),
            "not done after 2 attempts: `cargo test` still exits 101 — 3 failed"
        );
    }

    #[test]
    fn a_precheck_says_each_surprise_on_its_own_line() {
        let cause = TaskCause::PreCheck {
            found: NonEmpty::from((
                Surprise::TrivialCriterion {
                    cmd: "true".to_string(),
                },
                vec![Surprise::BrokenGuard {
                    cmd: "test -f seed".to_string(),
                    proves: None,
                }],
            )),
        };
        assert_eq!(
            sentence(&cause),
            "criterion `true` already passes before any work — the criteria need fixing, not \
             the task\nguard `test -f seed` is already red before any work started"
        );
    }

    #[test]
    fn a_refused_session_says_its_message_and_its_cause() {
        let cause = TaskCause::SessionRefused(yunta_core::events::SessionRefusal {
            adapter: yunta_core::AdapterId::from_static("mock"),
            cause: yunta_core::events::RefusalCause::Authentication,
            message: "Invalid API key".to_string(),
        });
        assert_eq!(
            sentence(&cause),
            "session `mock` refused the request (its credential was refused): Invalid API key"
        );
    }
}
