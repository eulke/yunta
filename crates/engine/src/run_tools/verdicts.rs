//! How an answer to a tool call reads.
//!
//! Every verdict a session gets back is written here, so a refusal about
//! a document, a finding or a file arrives in one shape: what was not
//! accepted, then a numbered list of what to fix, in the words the
//! document's own diagnostics use. An agent that learns to read one
//! refusal can read them all, and a rule reworded in the core is reworded
//! everywhere at once.
//!
//! Every answer, a refusal or not, is a [`Reply`]: the verdict first,
//! then each problem standing in the way, then what to call next — and
//! what a tool answers as data is JSON, written by [`json`].
//!
//! An acceptance says what the engine *read*, not merely that it parsed:
//! a tasks document that comes back as six tasks when the session meant seven is
//! a mistake only the session can still fix.

use serde::Serialize;
use yunta_core::diagnostic::{ArtifactFailure, Report};
use yunta_core::events::FindingOperation;
use yunta_core::expansion::{Decision, Denial};

use super::session::RunToolError;
use crate::process::Printed;
use crate::task_cycle::{CriterionRun, Judgement};

/// What a session is told back, in the one shape every tool answers in:
/// the verdict first, then each problem standing in its way, numbered,
/// then what to call next. An agent that has read one answer can read
/// them all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Reply {
    verdict: String,
    problems: Vec<String>,
    next: Option<String>,
}

impl Reply {
    /// An answer that says `verdict`.
    pub(super) fn new(verdict: impl Into<String>) -> Self {
        Reply {
            verdict: verdict.into(),
            problems: Vec::new(),
            next: None,
        }
    }

    /// The same answer, with what stands in the way, in order.
    pub(super) fn problems(mut self, problems: impl IntoIterator<Item = String>) -> Self {
        self.problems.extend(problems);
        self
    }

    /// The same answer, saying what to do next.
    pub(super) fn next(mut self, next: impl Into<String>) -> Self {
        self.next = Some(next.into());
        self
    }

    /// The text the session reads.
    pub(super) fn text(&self) -> String {
        let mut text = self.verdict.clone();
        for (position, problem) in self.problems.iter().enumerate() {
            text.push_str(&format!("\n\n  {}. {problem}", position + 1));
        }
        if let Some(next) = &self.next {
            text.push_str(&format!("\n\nNext: {next}"));
        }
        text
    }
}

/// What a tool answers as data, as the session reads it.
pub(super) fn json(answer: &impl Serialize) -> Result<String, RunToolError> {
    serde_json::to_string_pretty(answer).map_err(|source| RunToolError::Render { source })
}

/// One document's problems as an instruction to whoever offered it:
/// what was not accepted, and a numbered list of what to fix.
pub(super) fn numbered(heading: String, report: &Report) -> String {
    Reply::new(heading)
        .problems(report.diagnostics.iter().map(ToString::to_string))
        .text()
}

/// How a verdict about a file that is there and unreadable opens.
pub(super) fn failure_heading(report: &Report) -> String {
    format!("the {} it holds cannot be read:", report.document.label())
}

/// What a session is told when its work would not close, and how to go on:
/// the problems standing in the way, numbered, then the fix. `request_tool`
/// names the tool that asks for scope; it is empty when the session is not
/// offered that tool, and the sentence that names it is then left out. A
/// request the project denied is named as denied, since its answer was given
/// in the call that made it.
pub(super) fn stop_text(judgement: &Judgement, request_tool: &str) -> String {
    let mut text = Reply::new(
        "yunta: not done yet — this is your work judged as the engine will judge it when your \
         session ends, and it would not close:",
    )
    .problems(stop_problems(judgement))
    .text();
    text.push_str("\n\nFix what fails, then finish.");
    match judgement.request.as_ref().map(|outcome| &outcome.decision) {
        Some(Decision::Deny(denial)) => text.push_str(&format!(
            " Your request was denied in its call: {denial}. Keep your work within its scope."
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
            yunta_core::text::listed(judgement.waiting.iter().map(String::as_str))
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
            yunta_core::text::listed(paths.iter().map(String::as_str))
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

/// A request the project settles in the call: the denial, and what the
/// session does next with its work.
pub(super) fn denied(denial: &Denial, check_tool: &str) -> String {
    Reply::new(format!("request denied: {denial}"))
        .next(format!(
            "keep your work within its scope; `{check_tool}` tells you whether your task closes \
             as it stands"
        ))
        .text()
}

/// A request written for the attempt's end to decide, and what comes next.
pub(super) fn recorded(next: &str) -> String {
    Reply::new("request recorded — it is decided when this attempt ends")
        .next(next)
        .text()
}

/// A request whose new paths joined the one already waiting for the
/// attempt's end.
pub(super) fn amended(added: usize, next: &str) -> String {
    Reply::new(format!(
        "request amended — {} added to the request waiting for this attempt to end",
        yunta_core::text::counted(added, "new path")
    ))
    .next(next)
    .text()
}

/// A criterion the session declared unpassable, which its task does not
/// close on.
pub(super) fn declared(next: &str) -> String {
    Reply::new(
        "criterion declared unpassable — no work of the plan can pass it, so your task does not \
         close on it",
    )
    .next(next)
    .text()
}

/// A second declaration in one session: answered, and not recorded.
pub(super) fn already_declared(next: &str) -> String {
    Reply::new(
        "criterion not declared — this session already declared one unpassable, and one \
         declaration stands per session",
    )
    .next(next)
    .text()
}

pub(super) fn submission_refusal(report: &Report, name: &str) -> String {
    numbered(
        format!(
            "The {} `{name}` was not accepted. Fix these and submit again:",
            report.document.label()
        ),
        report,
    )
}

pub(super) fn refusal(operation: FindingOperation, report: &Report) -> String {
    let heading = match operation {
        FindingOperation::Post => "The finding was not accepted. Fix these and post it again:",
        FindingOperation::Update => {
            "The finding update was not accepted. Fix these and update it again:"
        }
        FindingOperation::Withdraw => {
            "The withdrawal was not accepted. Fix these and withdraw it again:"
        }
    };
    numbered(heading.to_string(), report)
}

/// What the engine read out of an artifact, so a session sees its meaning
/// survived the parse and not only its syntax.
pub(super) fn read_as(verified: &crate::artifacts::VerifiedArtifact) -> String {
    use crate::artifacts::ArtifactContent;
    match &verified.content {
        ArtifactContent::Opaque => "Verified by existence and content hash.".to_string(),
        ArtifactContent::Tasks(tasks) => counted_names(
            tasks.tasks.len(),
            "task",
            "registered",
            tasks.tasks.iter().map(|t| t.id.to_string()),
        ),
        ArtifactContent::Spec(spec) => counted_names(
            spec.specs.len(),
            "task",
            "specified",
            spec.specs.iter().map(|spec| spec.task.to_string()),
        ),
        ArtifactContent::Findings(findings) => counted_names(
            findings.len(),
            "finding",
            "posted",
            findings.iter().map(|f| f.id.to_string()),
        ),
        ArtifactContent::Questions(questions) => counted_names(
            questions.len(),
            "question",
            "to answer",
            questions.iter().map(|q| q.id.to_string()),
        ),
        ArtifactContent::Answers(answers) => counted_names(
            answers.len(),
            "question",
            "answered",
            answers.iter().map(|answer| answer.id.to_string()),
        ),
    }
}

/// `2 questions to answer: `q-a`, `q-b`` — how many of what, what
/// happened to them, and which ones, through the workspace's one
/// counter and its one joiner.
fn counted_names(
    how_many: usize,
    noun: &str,
    happened: &str,
    ids: impl Iterator<Item = String>,
) -> String {
    let ids: Vec<String> = ids.collect();
    let listed = match ids.is_empty() {
        true => "none".to_string(),
        false => yunta_core::text::listed(ids.iter().map(String::as_str)),
    };
    format!(
        "{} {happened}: {listed}",
        yunta_core::text::counted(how_many, noun)
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::process::Printed;
    use crate::scope::ScopeCheckResult;
    use crate::scope_expansion::ScopeExpansionOutcome;
    use crate::task_cycle::CriterionRun;
    use yunta_core::RunTool;

    #[test]
    fn a_denied_request_opens_with_its_denial_and_names_the_check_tool() {
        use yunta_core::RunTool;
        let check = RunTool::CheckTask.name();
        let text = denied(&Denial::ModeDeny, check);
        assert!(text.starts_with("request denied: "), "{text}");
        assert!(text.contains(&format!("`{check}`")), "{text}");
    }

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
        let text = stop_text(&judged("missing a.txt", None, &[]), request_tool);
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
        let text = stop_text(
            &judged("missing a.txt", Some(denied), &["b.txt"]),
            request_tool,
        );
        assert!(
            text.contains("Your request was denied in its call: scope_expansion mode is deny"),
            "{text}"
        );
        assert!(!text.contains("the engine answers it once"), "{text}");
    }

    #[test]
    fn an_amended_request_counts_the_paths_it_added() {
        assert!(amended(2, "next").starts_with("request amended — 2 new paths "));
        assert!(amended(1, "next").starts_with("request amended — 1 new path "));
    }
}
