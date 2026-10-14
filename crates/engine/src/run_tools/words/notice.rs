//! The sentences a session is told ahead of any call about its node's
//! artifacts and its task, and the sentences that follow a person's answer to
//! what it asked: the scope it was granted or refused, a review, a departure
//! from the plan, and tests a person accepted are wrong.

use std::path::PathBuf;

use yunta_core::events::{DeviationResolvedPayload, ScopeAnswer};
use yunta_core::ArtifactKind;

use super::scope::denial;
use crate::task_cycle::{Respecify, Review};

/// What a node that writes the run's spec does with tests a person accepted
/// are wrong.
const WRITE_AGAIN: &str = "Write those tasks' tests again, as the departures and the person \
                           say, and hand the spec over again — every other task's spec as it \
                           is: one that changes another task's tests, or gives a task the tests \
                           it had, is refused.";

/// The names this session's CLI gives the run's tools, beside the names any
/// text may give them, each with the tool it reaches.
pub(crate) fn naming_text(renamed: &[(String, &str)]) -> String {
    let mut text = String::from(
        "\n\nThis run's tools reach you under names of your CLI's own. Call each by the \
         name it has here, whatever any text calls it:",
    );
    for (called, tool) in renamed {
        text.push_str(&format!("\n  `{called}` — {tool}"));
    }
    text
}

/// The documents a node declares under a kind that has a submission tool,
/// each paired with the tool that takes it.
pub(crate) fn documents_text(submitted: &[(ArtifactKind, String)]) -> String {
    let mut text = String::from(
        "\n\nSubmit each document this node declares with its run tool, as a structured \
         object — never as a file:",
    );
    for (kind, tool) in submitted {
        text.push_str(&format!("\n  the `{kind}` document → `{tool}`"));
    }
    text.push_str(
        "\nThe tool validates the document exactly as the node's close will and answers \
         at once. A refusal names every problem to fix — fix them and submit again, as \
         many times as it takes; it costs the run nothing. An acceptance reports what \
         the engine read and writes the file. A document never submitted fails the node.",
    );
    text
}

/// The findings artifact a node declares: a file the session never writes,
/// because the engine derives it from what the session reported.
pub(crate) fn findings_text(
    kind: ArtifactKind,
    post: &str,
    update: &str,
    withdraw: &str,
) -> String {
    format!(
        "\n\nReport each finding with `{post}` the moment you see it — one call per \
         finding, never a file. The engine writes this node's `{kind}` artifact at the \
         end from everything this node reported; a session that reports nothing yields \
         an empty list. A refusal names what to fix in that one finding — fix it and \
         post it again; the others already reported are kept. To change a finding you \
         reported, `{update}` with the same id and the whole finding; to take one back, \
         `{withdraw}` with its id and why. A withdrawn id is final."
    )
}

/// The plain files a node declares, and where they go: this node's own
/// directory, and the tool that confirms a file is there.
pub(crate) fn files_text(dir: &str, names: &[&str], check: &str) -> String {
    let mut text = format!(
        "\n\nWrite each file this node declares under {dir} — this node's own \
         directory, which nothing else writes:"
    );
    for name in names {
        text.push_str(&format!("\n  `{name}`"));
    }
    text.push_str(&format!(
        "\nYou may call `{check}` to confirm a file is there before this session ends."
    ));
    text
}

/// What a task session is told about its task: where its contract is read,
/// what it may not change, and how its work is judged when it ends.
pub(crate) fn task_text(read: &str, check: &str, plan: &str, approved: &str) -> String {
    format!(
        "\n\nRead this task's scope, criteria and notes with `{read}` before you change \
         anything — the tasks document is not in your checkout, and the same call shows \
         what earlier attempts left red.{plan}{approved} When your session ends the engine \
         runs every criterion and rejects any change outside the scope; `{check}` judges \
         your work exactly that way, so call it before you finish."
    )
}

/// The tests a person approved for a task, which its session is told are not
/// its to change.
pub(crate) fn approved_tests_text(files: &str, depart: &str) -> String {
    format!(
        " The tests a person approved for this task are in your checkout — {files} — and are \
         not yours to change: the engine refuses a write to them. One you believe is \
         wrong is a departure from the plan: declare it on that criterion with \
         `{depart}`, and a person decides whether it is written again."
    )
}

/// The plan a task belongs to, as the session of the task is told it carries it.
pub(crate) fn plan_text(depart: &str) -> String {
    format!(
        " It also carries the plan the task belongs to: the design the task names, and \
         the other tasks, which own what your scope leaves out. Build the design as the \
         plan declares it; where your task cannot, ask for the scope it needs, or declare \
         the departure with `{depart}` — never build something else and say nothing."
    )
}

/// What a session is told when a person's review sent it back to its work
/// and the session can be picked back up.
pub(crate) fn review_continued_text(review: &Review) -> String {
    format!(
        "{} Revise it accordingly and hand it over again the way you did before.",
        reviewed(review, "you")
    )
}

/// What a fresh session of a node is told when a person's review sent the
/// run back to it and the session that did the work cannot be picked back
/// up: the review, and where what the node handed over is.
pub(crate) fn review_fresh_text(review: &Review, handed: &[PathBuf]) -> String {
    format!(
        "\n\nThis node already ran in this run. {}{} Revise it accordingly and hand it \
         over again.",
        reviewed(review, "this node"),
        handed_review(handed)
    )
}

/// What a session that departed from the plan is told when a person
/// answered: what they decided, in their words, and what to do from here.
pub(crate) fn departure_text(answer: &DeviationResolvedPayload) -> String {
    let said = answer
        .said
        .as_deref()
        .map(|said| format!(": {}", said.trim()))
        .unwrap_or_default();
    match (answer.accepted, &answer.respecified_by) {
        (true, Some(_)) => format!(
            "A person accepted the departure from the plan you declared{said}, and your task's \
             tests were written again from it. Finish the task on the work as it stands, held \
             to its tests as they are now."
        ),
        (true, None) => format!(
            "A person accepted the departure from the plan you declared{said}. Finish the \
             task on the work as it stands."
        ),
        (false, _) => format!(
            "A person sent back the departure from the plan you declared{said}. Do what they \
             say — or, where they say nothing more, build what the plan declares; declare a \
             departure again only for what still cannot be built as they ask."
        ),
    }
}

/// What a session is told when it is picked back up after a person accepted
/// that tests it wrote are wrong: the tests as they are written again, and
/// what to do with them.
pub(crate) fn respecify_continued_text(respecify: &Respecify) -> String {
    format!("{} {WRITE_AGAIN}", respecified(respecify, "you"))
}

/// What a fresh session of the node that wrote the run's spec is told
/// when a person accepted that tests it wrote are wrong: the departures,
/// what they said, and where what the node handed over is.
pub(crate) fn respecify_fresh_text(respecify: &Respecify, handed: &[PathBuf]) -> String {
    format!(
        "\n\nThis node already ran in this run. {}{} {WRITE_AGAIN}",
        respecified(respecify, "this node"),
        handed_at(handed)
    )
}

/// What a session is told after the answer to the scope it asked for: the
/// answer, and the reads it is given, when its tools are mounted.
pub(crate) fn scope_answer_text(answer: &ScopeAnswer, read: Option<&str>) -> String {
    format!(
        "{}{} The work you did is still in your checkout; continue from where you stopped.",
        scope_answered(answer),
        read.unwrap_or_default()
    )
}

/// Which tools a task's session reads its scope with, and checks its work with.
pub(crate) fn scope_read_task_text(read: &str, check: &str) -> String {
    format!(
        " `{read}` shows your task's scope and what still keeps it from closing; call \
         `{check}` before you finish."
    )
}

/// Which tool a node's session checks its scope with.
pub(crate) fn scope_read_node_text(check: &str) -> String {
    format!(" `{check}` shows what your close would find outside your scope.")
}

/// What the answer to a scope request says: what was granted, or refused and why.
fn scope_answered(answer: &ScopeAnswer) -> String {
    match answer {
        ScopeAnswer::Granted(paths) => format!(
            "The scope you asked for was granted: {}. You may write there now.",
            yunta_core::listed_globs(paths)
        ),
        ScopeAnswer::Denied(Some(refused)) => format!(
            "The scope you asked for was refused: {}. Stay within the scope you have.",
            denial(refused)
        ),
        ScopeAnswer::Denied(None) => {
            "The scope you asked for was refused. Stay within the scope you have.".to_string()
        }
    }
}

/// The tests a person accepted are wrong, task by task: what each session
/// departed from, and what the person said.
fn respecified(respecify: &Respecify, whose: &str) -> String {
    let mut text = format!(
        "A person accepted that tests {whose} wrote are wrong, where a task departed from them:"
    );
    for owed in &respecify.owed {
        for departure in &owed.departures {
            text.push_str(&format!(
                "\n  task `{}` departs from {}: the plan says {}; its work instead {}; because {}",
                owed.task,
                departure.from,
                departure.planned.trim(),
                departure.instead.trim(),
                departure.why.trim()
            ));
        }
        if let Some(said) = &owed.said {
            text.push_str(&format!("\n  the person said: {}", said.trim()));
        }
    }
    text
}

/// The review, in the person's words.
fn reviewed(review: &Review, whose: &str) -> String {
    format!(
        "A person reviewed what {whose} handed over at gate `{}` and chose `{}`: {}",
        review.gate,
        review.option,
        review.said.trim()
    )
}

/// The paths a node handed over, each quoted and joined; `None` when it
/// handed over nothing.
fn handed_list(handed: &[PathBuf]) -> Option<String> {
    let at: Vec<String> = handed
        .iter()
        .map(|path| format!("`{}`", path.display()))
        .collect();
    (!at.is_empty()).then(|| at.join(", "))
}

/// Where what a node handed over is, as a sentence; empty when it handed
/// over nothing.
fn handed_at(handed: &[PathBuf]) -> String {
    handed_list(handed)
        .map(|at| format!("\nWhat it handed over is at {at}."))
        .unwrap_or_default()
}

/// Where what a node handed over is, as the review's sentence; empty when it
/// handed over nothing.
fn handed_review(handed: &[PathBuf]) -> String {
    handed_list(handed)
        .map(|at| format!(" What it handed over is at {at}."))
        .unwrap_or_default()
}
