//! The sentences a session reads when its stop is refused: what stands
//! between its work and its close, numbered, and how to go on.

use yunta_core::diagnostic::ArtifactFailure;
use yunta_core::expansion::Decision;
use yunta_core::text::listed;

use super::scope::denial;
use super::verdicts::failure_heading;
use crate::process::Printed;
use crate::run_tools::verdicts::{numbered, Reply};
use crate::task_cycle::{CriterionRun, Judgement};

/// What a session is told when its work would not close, and how to go on:
/// the problems standing in the way, numbered, then the fix. `request_tool`
/// names the tool that asks for scope; it is empty when the session is not
/// offered that tool, and the sentence that names it is then left out. A
/// request the project denied is named as denied, since its answer was given
/// in the call that made it.
pub(crate) fn stop_unfinished(judgement: &Judgement, request_tool: &str) -> String {
    let mut text = Reply::new(
        "yunta: not done yet — this is your work judged as the engine will judge it when your \
         session ends, and it would not close:",
    )
    .problems(stop_problems(judgement))
    .text();
    text.push_str("\n\nFix what fails, then finish.");
    match judgement.request.as_ref().map(|outcome| &outcome.decision) {
        Some(Decision::Deny(refusal)) => text.push_str(&format!(
            " Your request was denied in its call: {}. Keep your work within its scope.",
            denial(refusal)
        )),
        _ if !request_tool.is_empty() => text.push_str(&format!(
            " If the fix needs a file outside your scope, ask for it with `{request_tool}` and \
             finish; the engine answers it once your session ends."
        )),
        _ => {}
    }
    text.push_str(" Finishing again without changing anything ends the session as it stands.");
    text
}

/// A criterion that exits red, as the session reads it: its command and its
/// exit, then the last lines it printed, which say why it fails.
fn red_problem(run: &CriterionRun) -> String {
    let problem = format!("`{}` exits {}, and must exit 0", run.cmd, run.exit_code);
    let tail = run.output.as_ref().map(Printed::tail).unwrap_or_default();
    if tail.is_empty() {
        return problem;
    }
    let lines: Vec<String> = tail.iter().map(|line| format!("     {line}")).collect();
    format!("{problem}. Its last lines:\n{}", lines.join("\n"))
}

/// What stands between the work and its close, one problem each, in the
/// order the session reads them: the criteria that are red, the guards
/// waiting on them, the paths outside the scope, and the documents a node owes.
fn stop_problems(judgement: &Judgement) -> Vec<String> {
    let mut problems: Vec<String> = judgement
        .criteria
        .iter()
        .filter(|run| run.exit_code != 0)
        .map(red_problem)
        .collect();
    if !judgement.waiting.is_empty() {
        problems.push(format!(
            "not run yet, because a criterion before it is red: {}",
            listed(judgement.waiting.iter().map(String::as_str))
        ));
    }
    if !judgement.scope.violations.is_empty() {
        let paths: Vec<String> = judgement
            .scope
            .violations
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        problems.push(format!(
            "changed outside the scope — undo these changes: {}",
            listed(paths.iter().map(String::as_str))
        ));
    }
    problems.extend(
        judgement
            .documents
            .iter()
            .map(|fault| document_problem(&fault.spec, &fault.failure)),
    );
    problems
}

/// A document a session hands over that the close would refuse, as the
/// session reads it: its numbered problems when the file is there, and what
/// the failure says otherwise.
fn document_problem(spec: &str, failure: &ArtifactFailure) -> String {
    match failure.report() {
        Some(report) => numbered(format!("{spec} — {}", failure_heading(report)), report),
        None => format!("{spec} — {failure}"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use yunta_core::expansion::{Decision, Denial};
    use yunta_core::RunTool;

    use super::stop_unfinished;
    use crate::process::Printed;
    use crate::scope::ScopeCheckResult;
    use crate::scope_expansion::ScopeExpansionOutcome;
    use crate::task_cycle::{CriterionRun, Judgement};

    /// A judgement of one red criterion that printed `tail`, with `request`
    /// and `outside` as the stop's other findings.
    fn judged(tail: &str, request: Option<ScopeExpansionOutcome>, outside: &[&str]) -> Judgement {
        Judgement {
            criteria: vec![CriterionRun {
                cmd: "test -f a.txt".into(),
                exit_code: 1,
                is_guard: false,
                reused: false,
                duration_ms: None,
                output: Some(Printed::Recorded {
                    object: None,
                    tail: vec![tail.to_string()],
                    conclusion: None,
                }),
                tree: None,
                head: None,
            }],
            waiting: Vec::new(),
            scope: ScopeCheckResult {
                violations: outside.iter().map(PathBuf::from).collect(),
                ..Default::default()
            },
            request,
            documents: Vec::new(),
            unpassable: None,
            tree: Some(yunta_core::TreeId::from_static("deadbeef")),
        }
    }

    #[test]
    fn a_red_criterion_is_told_with_the_last_lines_it_printed() {
        let request_tool = RunTool::RequestScopeExpansion.name();
        let text = stop_unfinished(&judged("missing a.txt", None, &[]), request_tool);
        assert!(
            text.contains("`test -f a.txt` exits 1, and must exit 0"),
            "{text}"
        );
        assert!(text.contains("missing a.txt"), "{text}");
    }

    #[test]
    fn a_request_denied_in_its_call_is_said_so_in_the_stop_text() {
        let request_tool = RunTool::RequestScopeExpansion.name();
        let denied = ScopeExpansionOutcome {
            asked: None,
            precheck: None,
            decision: Decision::Deny(Denial::ModeDeny),
            count: 0,
        };
        let text = stop_unfinished(
            &judged("missing a.txt", Some(denied), &["b.txt"]),
            request_tool,
        );
        assert!(
            text.contains("Your request was denied in its call: scope_expansion mode is deny"),
            "{text}"
        );
        assert!(!text.contains("the engine answers it once"), "{text}");
    }
}
