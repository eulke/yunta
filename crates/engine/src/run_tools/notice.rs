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

use yunta_core::{ArtifactKind, ArtifactSpec};

use super::listener::RunToolsSession;

/// What a session is told about the artifacts its node declares: which
/// documents to submit and through which tool, which findings to report
/// as it sees them, and which files to write. `None` when there is
/// nothing mounted, or nothing declared for it to act on.
pub(crate) fn submission_notice(
    session: Option<&RunToolsSession>,
    declared: &[ArtifactSpec],
    artifact_dir: Option<&std::path::Path>,
) -> Option<String> {
    session?;
    let text: String = [
        documents_to_submit(declared),
        findings_to_report(declared),
        files_to_write(declared, artifact_dir),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!text.is_empty()).then_some(text)
}

/// The documents this node declares under a kind that has a submission
/// tool, each paired with the tool that takes it.
fn documents_to_submit(declared: &[ArtifactSpec]) -> Option<String> {
    let submitted: Vec<(&str, &str)> = declared
        .iter()
        .filter_map(|spec| match spec {
            ArtifactSpec::Typed { name, kind } => {
                kind.submit_tool().map(|tool| (name.as_str(), tool))
            }
            ArtifactSpec::Plain(_) => None,
        })
        .collect();
    if submitted.is_empty() {
        return None;
    }
    let mut text = String::from(
        "\n\nSubmit each document this node declares with its run tool, as a structured \
         object — never as a file:",
    );
    for (name, tool) in submitted {
        text.push_str(&format!("\n  `{name}` → `{tool}`"));
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
fn findings_to_report(declared: &[ArtifactSpec]) -> Option<String> {
    let accumulated: Vec<&str> = declared
        .iter()
        .filter_map(|spec| match spec {
            ArtifactSpec::Typed { name, kind } if kind.submit_tool().is_none() => {
                Some(name.as_str())
            }
            _ => None,
        })
        .collect();
    let name = accumulated.first()?;
    Some(format!(
        "\n\nReport each finding with `{post}` the moment you see it — one call per \
         finding, never a file. The engine writes `{name}` at the end from everything \
         this node reported; a session that reports nothing yields an empty list. A \
         refusal names what to fix in that one finding — fix it and post it again; the \
         others already reported are kept. To change a finding you reported, \
         `{update}` with the same id and the whole finding; to take one back, \
         `{withdraw}` with its id and why. A withdrawn id is final.",
        post = ArtifactKind::POST_FINDING_TOOL,
        update = ArtifactKind::UPDATE_FINDING_TOOL,
        withdraw = ArtifactKind::WITHDRAW_FINDING_TOOL,
    ))
}

/// The plain files this node declares, and where they go — told only
/// when the session was granted the directory they belong in, since a
/// session that cannot reach it has nothing to act on.
fn files_to_write(
    declared: &[ArtifactSpec],
    artifact_dir: Option<&std::path::Path>,
) -> Option<String> {
    let written: Vec<&str> = declared
        .iter()
        .filter_map(|spec| match spec {
            ArtifactSpec::Plain(name) => Some(name.as_str()),
            ArtifactSpec::Typed { .. } => None,
        })
        .collect();
    if written.is_empty() {
        return None;
    }
    let dir = artifact_dir?;
    let mut text = format!(
        "\n\nWrite each file this node declares under {}:",
        dir.display()
    );
    for name in written {
        text.push_str(&format!("\n  `{name}`"));
    }
    text.push_str(
        "\nYou may call `yunta_check_artifact` to confirm a file is there before \
         this session ends.",
    );
    Some(text)
}
