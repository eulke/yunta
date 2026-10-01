//! What a session is told about its node's artifacts before it calls
//! anything.
//!
//! Telling an agent to call a tool is a promise, and a promise is only
//! keepable where the tool was actually mounted. The notice is produced
//! from the mount itself — rather than beside the published contract,
//! which is assembled before any runner is resolved — which is what makes
//! "instructed but not mounted" unrepresentable instead of merely
//! avoided: an adapter that declares no `run_tools` opens no session, and
//! no session produces no sentence.
//!
//! The three paragraphs are the three ways an artifact comes to exist: a
//! document the session submits, findings the engine accumulates from
//! what the session reports, and a plain file the session writes itself.
//! A node that declares nothing of a kind hears nothing about it.

use yunta_core::{ArtifactKind, ArtifactSpec, RunTool};

use super::listener::RunToolsSession;

/// The names this session's CLI gives the run's tools, beside the names
/// any text — an author's prompt included — may give them. `None` when
/// nothing is mounted, or the CLI calls each by its own name.
pub(crate) fn naming_notice(session: Option<&RunToolsSession>) -> Option<String> {
    let renamed = session?.renamed();
    if renamed.is_empty() {
        return None;
    }
    let mut text = String::from(
        "\n\nThis run's tools reach you under names of your CLI's own. Call each by the \
         name it has here, whatever any text calls it:",
    );
    for (called, tool) in renamed {
        text.push_str(&format!("\n  `{called}` — {tool}"));
    }
    Some(text)
}

/// What a session is told about the artifacts its node declares: which
/// documents to submit and through which tool, which findings to report
/// as it sees them, and which files to write. `None` when there is
/// nothing mounted, or nothing declared for it to act on.
pub(crate) fn submission_notice(
    session: Option<&RunToolsSession>,
    declared: &[ArtifactSpec],
    artifact_dir: Option<&std::path::Path>,
) -> Option<String> {
    let session = session?;
    let text: String = [
        documents_to_submit(session, declared),
        findings_to_report(session, declared),
        files_to_write(session, declared, artifact_dir),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!text.is_empty()).then_some(text)
}

/// What a task session is told about its task: where its contract is
/// read, and how its work is judged before the session ends. `None` when
/// nothing is mounted, or the session works no task.
pub(crate) fn task_notice(
    session: Option<&RunToolsSession>,
    task: Option<&super::host::TaskAccess>,
) -> Option<String> {
    let session = session?;
    let task = task?;
    let plan = match task.plan {
        Some(_) => format!(
            " It also carries the plan the task belongs to: the design the task names, and \
             the other tasks, which own what your scope leaves out. Build the design as the \
             plan declares it; where your task cannot, ask for the scope it needs, or declare \
             the departure with `{depart}` — never build something else and say nothing.",
            depart = session.called(RunTool::DeclareDeviation),
        ),
        None => String::new(),
    };
    Some(format!(
        "\n\nRead this task's scope, criteria and notes with `{read}` before you change \
         anything — the tasks document is not in your checkout, and the same call shows \
         what earlier attempts left red.{plan} When your session ends the engine runs every \
         criterion and rejects any change outside the scope; `{check}` judges your work \
         exactly that way, so call it before you finish.",
        read = session.called(RunTool::Task),
        check = session.called(RunTool::CheckTask),
    ))
}

/// The documents this node declares under a kind that has a submission
/// tool, each paired with the tool that takes it.
fn documents_to_submit(session: &RunToolsSession, declared: &[ArtifactSpec]) -> Option<String> {
    let submitted: Vec<(ArtifactKind, String)> = declared
        .iter()
        .filter_map(|spec| match spec {
            ArtifactSpec::Interpreted(kind) => kind
                .submit_tool()
                .map(|_| (*kind, session.called(RunTool::Submit(*kind)))),
            ArtifactSpec::Opaque(_) => None,
        })
        .collect();
    if submitted.is_empty() {
        return None;
    }
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
    Some(text)
}

/// The findings artifact this node declares — a file the session never
/// writes, because the engine derives it from what the session reported.
fn findings_to_report(session: &RunToolsSession, declared: &[ArtifactSpec]) -> Option<String> {
    let kind = declared.iter().find_map(|spec| match spec {
        ArtifactSpec::Interpreted(kind) if kind.submit_tool().is_none() => Some(*kind),
        _ => None,
    })?;
    Some(format!(
        "\n\nReport each finding with `{post}` the moment you see it — one call per \
         finding, never a file. The engine writes this node's `{kind}` artifact at the \
         end from everything this node reported; a session that reports nothing yields \
         an empty list. A refusal names what to fix in that one finding — fix it and \
         post it again; the others already reported are kept. To change a finding you \
         reported, `{update}` with the same id and the whole finding; to take one back, \
         `{withdraw}` with its id and why. A withdrawn id is final.",
        post = session.called(RunTool::PostFinding),
        update = session.called(RunTool::UpdateFinding),
        withdraw = session.called(RunTool::WithdrawFinding),
    ))
}

/// The plain files this node declares, and where they go — this node's
/// own directory, told only when the session was granted it, since a
/// session that cannot reach it has nothing to act on.
fn files_to_write(
    session: &RunToolsSession,
    declared: &[ArtifactSpec],
    artifact_dir: Option<&std::path::Path>,
) -> Option<String> {
    let written: Vec<&str> = declared
        .iter()
        .filter_map(|spec| match spec {
            ArtifactSpec::Opaque(name) => Some(name.as_str()),
            ArtifactSpec::Interpreted(_) => None,
        })
        .collect();
    if written.is_empty() {
        return None;
    }
    let dir = artifact_dir?;
    let mut text = format!(
        "\n\nWrite each file this node declares under {} — this node's own \
         directory, which nothing else writes:",
        dir.display()
    );
    for name in written {
        text.push_str(&format!("\n  `{name}`"));
    }
    text.push_str(&format!(
        "\nYou may call `{}` to confirm a file is there before this session ends.",
        session.called(RunTool::CheckArtifact)
    ));
    Some(text)
}

/// Whose scope an answer widened or held: a loop's task, or a node's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asker {
    Task,
    Node,
}

/// What a session is told when it is picked back up after a person
/// answered it: the answer, and what to do with it.
pub(crate) fn continuation_notice(
    session: Option<&RunToolsSession>,
    answer: &crate::task_cycle::Answer,
    asker: Asker,
) -> String {
    match answer {
        crate::task_cycle::Answer::Scope(scope) => scope_notice(session, scope, asker),
        crate::task_cycle::Answer::Review(review) => format!(
            "{} Revise it accordingly and hand it over again the way you did before.",
            reviewed(review, "you")
        ),
        crate::task_cycle::Answer::Deviation(answer) => departure_notice(answer),
    }
}

/// What a session that departed from the plan is told when a person
/// answered: what they decided, in their words, and what to do now.
fn departure_notice(answer: &yunta_core::events::DeviationResolvedPayload) -> String {
    let said = answer
        .said
        .as_deref()
        .map(|said| format!(": {}", said.trim()))
        .unwrap_or_default();
    match answer.accepted {
        true => format!(
            "A person accepted the departure from the plan you declared{said}. Finish the \
             task on the work as it stands."
        ),
        false => format!(
            "A person sent back the departure from the plan you declared{said}. Do what they \
             say — or, where they say nothing more, build what the plan declares; declare a \
             departure again only for what still cannot be built as they ask."
        ),
    }
}

/// What a fresh session of a node is told when a person's review sent
/// the run back to it and the session that did the work cannot be picked
/// back up: the review, and where what the node handed over is, which it
/// reads rather than being given a copy of.
pub(crate) fn fresh_review_notice(
    review: &crate::task_cycle::Review,
    handed: &[std::path::PathBuf],
) -> String {
    let at: Vec<String> = handed
        .iter()
        .map(|path| format!("`{}`", path.display()))
        .collect();
    let at = match at.is_empty() {
        true => String::new(),
        false => format!(" What it handed over is at {}.", at.join(", ")),
    };
    format!(
        "\n\nThis node already ran in this run. {}{at} Revise it accordingly and hand it \
         over again.",
        reviewed(review, "this node")
    )
}

/// The review, in the person's words.
fn reviewed(review: &crate::task_cycle::Review, whose: &str) -> String {
    format!(
        "A person reviewed what {whose} handed over at gate `{}` and chose `{}`: {}",
        review.gate,
        review.option,
        review.said.trim()
    )
}

/// What a session is told when it is picked back up after the answer to
/// the scope it asked for: the answer, that its work is where it left
/// it, and — when the run tools are mounted — which tool reads what it
/// may now do. The scope itself is never copied here: the tools read it
/// from the same log the fence and the close read, so the session and
/// the engine cannot hold two versions of it.
fn scope_notice(
    session: Option<&RunToolsSession>,
    answer: &yunta_core::events::ScopeAnswer,
    asker: Asker,
) -> String {
    use yunta_core::events::ScopeAnswer;
    let answered = match answer {
        ScopeAnswer::Granted(paths) => format!(
            "The scope you asked for was granted: {}. You may write there now.",
            yunta_core::listed_globs(paths)
        ),
        ScopeAnswer::Denied(Some(reason)) => format!(
            "The scope you asked for was refused: {reason}. Stay within the scope you have."
        ),
        ScopeAnswer::Denied(None) => {
            "The scope you asked for was refused. Stay within the scope you have.".to_string()
        }
    };
    let read = session.map(|session| match asker {
        Asker::Task => format!(
            " `{read}` shows your task's scope and what still keeps it from closing; call \
             `{check}` before you finish.",
            read = session.called(RunTool::Task),
            check = session.called(RunTool::CheckTask),
        ),
        Asker::Node => format!(
            " `{check}` shows what your close would find outside your scope.",
            check = session.called(RunTool::CheckScope),
        ),
    });
    format!(
        "{answered}{} The work you did is still in your checkout; continue from where you \
         stopped.",
        read.unwrap_or_default()
    )
}
