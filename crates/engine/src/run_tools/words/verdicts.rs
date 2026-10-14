//! The sentences of the verdicts a tool gives when it settles a request or a
//! declaration, the way a reply's own parts read, and the heading of a
//! refused finding: what each scope or departure answer says, in the shape
//! [`Reply`] gives it.

use yunta_core::diagnostic::Report;
use yunta_core::events::{FindingOperation, Unanswered};
use yunta_core::expansion::Denial;
use yunta_core::text::counted;

use super::scope::denial;
use crate::run_tools::verdicts::Reply;

/// A reply's text: the verdict, then each problem numbered, then what to
/// call next. The shape of a reply is [`Reply`]; the labels it writes are these.
pub(crate) fn reply_text(verdict: &str, problems: &[String], next: Option<&str>) -> String {
    let mut text = verdict.to_string();
    for (position, problem) in problems.iter().enumerate() {
        text.push_str(&format!("\n\n  {}. {problem}", position + 1));
    }
    if let Some(next) = next {
        text.push_str(&format!("\n\nNext: {next}"));
    }
    text
}

/// How a verdict about a file that is there and unreadable opens.
pub(super) fn failure_heading(report: &Report) -> String {
    format!("the {} it holds cannot be read:", report.document.label())
}

/// The heading of a refusal of a finding's operation: what was not
/// accepted, and what to do with the finding again.
pub(crate) fn finding_refusal_heading(operation: FindingOperation) -> &'static str {
    match operation {
        FindingOperation::Post => "The finding was not accepted. Fix these and post it again:",
        FindingOperation::Update => {
            "The finding update was not accepted. Fix these and update it again:"
        }
        FindingOperation::Withdraw => {
            "The withdrawal was not accepted. Fix these and withdraw it again:"
        }
    }
}

/// A request the project settles in the call: the denial, and what the
/// session does next with its work.
pub(crate) fn denied(refusal: &Denial, check_tool: &str) -> String {
    Reply::new(format!("request denied: {}", denial(refusal)))
        .next(format!(
            "keep your work within its scope; `{check_tool}` tells you whether your task closes \
             as it stands"
        ))
        .text()
}

/// A request written for the attempt's end to decide, and what comes next.
pub(crate) fn recorded(next: &str) -> String {
    Reply::new("request recorded — it is decided when this attempt ends")
        .next(next)
        .text()
}

/// A request whose new paths joined the one already waiting for the
/// attempt's end.
pub(crate) fn amended(added: usize, next: &str) -> String {
    Reply::new(format!(
        "request amended — {} added to the request waiting for this attempt to end",
        counted(added, "new path")
    ))
    .next(next)
    .text()
}

/// A criterion the session declared unpassable, which its task does not
/// close on, and what the attempt does next: the plan answers for it.
pub(crate) fn declared(criterion: &str) -> String {
    Reply::new(
        "criterion declared unpassable — no work of the plan can pass it, so your task does not \
         close on it",
    )
    .next(format!(
        "end this attempt; the plan, not more work on the task, answers for `{criterion}`"
    ))
    .text()
}

/// A second declaration in one session: answered, and not recorded.
pub(crate) fn already_declared() -> String {
    Reply::new(
        "criterion not declared — this session already declared one unpassable, and one \
         declaration stands per session",
    )
    .next("end this attempt; the declaration already made stands")
    .text()
}

/// A departure from the plan the session declared: recorded, and not closed
/// on, with what the session does next.
pub(crate) fn departure_recorded(check_tool: &str) -> String {
    Reply::new(
        "departure recorded — your task does not close on it: when this session ends a person \
         accepts it or sends it back with what to do instead",
    )
    .next(format!(
        "finish the rest of the task as the plan says; `{check_tool}` tells you whether it closes"
    ))
    .text()
}

/// The words an agent reads for a criterion that never answered: what
/// happened to its command, in the terms a person fixing it would use.
pub(crate) fn unanswered_words(why: Unanswered) -> &'static str {
    match why {
        Unanswered::NotFound => "command not found",
        Unanswered::NotExecutable => "command not executable",
        Unanswered::Stopped => "stopped before it answered",
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::expansion::Denial;
    use yunta_core::RunTool;

    use super::{amended, denied};

    #[test]
    fn a_denied_request_opens_with_its_denial_and_names_the_check_tool() {
        let check = RunTool::CheckTask.name();
        let text = denied(&Denial::ModeDeny, check);
        assert!(text.starts_with("request denied: "), "{text}");
        assert!(text.contains(&format!("`{check}`")), "{text}");
    }

    #[test]
    fn an_amended_request_counts_the_paths_it_added() {
        assert!(amended(2, "next").starts_with("request amended — 2 new paths "));
        assert!(amended(1, "next").starts_with("request amended — 1 new path "));
    }
}
