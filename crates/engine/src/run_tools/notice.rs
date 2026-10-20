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
//! A node that declares nothing of a kind hears nothing about it. The
//! sentences themselves are written in [`words`]; this module
//! decides which of them a session is told.

use std::path::PathBuf;

use yunta_core::events::ScopeAnswer;
use yunta_core::{ArtifactKind, ArtifactSpec, RunTool};

use super::listener::RunToolsSession;
use super::words;

/// The names this session's CLI gives the run's tools, beside the names
/// any text — an author's prompt included — may give them. `None` when
/// nothing is mounted, or the CLI calls each by its own name.
pub(crate) fn naming_notice(session: Option<&RunToolsSession>) -> Option<String> {
    let renamed = session?.renamed();
    if renamed.is_empty() {
        return None;
    }
    Some(words::naming_text(&renamed))
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
    tests: &[yunta_core::TestFile],
) -> Option<String> {
    let session = session?;
    let task = task?;
    let approved = match tests.is_empty() {
        true => String::new(),
        false => words::approved_tests_text(
            &yunta_core::text::listed(tests.iter().map(|file| file.path.as_str())),
            &session.called(RunTool::DeclareDeviation),
        ),
    };
    let plan = match task.plan {
        Some(_) => words::plan_text(&session.called(RunTool::DeclareDeviation)),
        None => String::new(),
    };
    Some(words::task_text(
        &session.called(RunTool::Task),
        &session.called(RunTool::CheckTask),
        &plan,
        &approved,
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
    Some(words::documents_text(&submitted))
}

/// The findings artifact this node declares — a file the session never
/// writes, because the engine derives it from what the session reported.
fn findings_to_report(session: &RunToolsSession, declared: &[ArtifactSpec]) -> Option<String> {
    let kind = declared.iter().find_map(|spec| match spec {
        ArtifactSpec::Interpreted(kind) if kind.submit_tool().is_none() => Some(*kind),
        _ => None,
    })?;
    Some(words::findings_text(
        kind,
        &session.called(RunTool::PostFinding),
        &session.called(RunTool::UpdateFinding),
        &session.called(RunTool::WithdrawFinding),
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
    Some(words::files_text(
        &dir.display().to_string(),
        &written,
        &session.called(RunTool::CheckArtifact),
    ))
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
        crate::task_cycle::Answer::Review(review) => words::review_continued_text(review),
        crate::task_cycle::Answer::Deviation(answer) => words::departure_text(answer),
        crate::task_cycle::Answer::Respecify(respecify) => {
            words::respecify_continued_text(respecify)
        }
    }
}

/// What a fresh session of the node that wrote the run's spec is told
/// when a person accepted that tests it wrote are wrong: the departures,
/// what they said, and where what the node handed over is.
pub(crate) fn fresh_respecify_notice(
    respecify: &crate::task_cycle::Respecify,
    handed: &[PathBuf],
) -> String {
    words::respecify_fresh_text(respecify, handed)
}

/// What a fresh session of a node is told when a person's review sent
/// the run back to it and the session that did the work cannot be picked
/// back up: the review, and where what the node handed over is, which it
/// reads rather than being given a copy of.
pub(crate) fn fresh_review_notice(
    review: &crate::task_cycle::Review,
    handed: &[PathBuf],
) -> String {
    words::review_fresh_text(review, handed)
}

/// What a session is told when it is picked back up after the answer to
/// the scope it asked for: the answer, that its work is where it left
/// it, and — when the run tools are mounted — which tool reads what it
/// may now do. The scope itself is never copied here: the tools read it
/// from the same log the fence and the close read, so the session and
/// the engine cannot hold two versions of it.
fn scope_notice(session: Option<&RunToolsSession>, answer: &ScopeAnswer, asker: Asker) -> String {
    let read = session.map(|session| match asker {
        Asker::Task => words::scope_read_task_text(
            &session.called(RunTool::Task),
            &session.called(RunTool::CheckTask),
        ),
        Asker::Node => words::scope_read_node_text(&session.called(RunTool::CheckScope)),
    });
    words::scope_answer_text(answer, read.as_deref())
}
