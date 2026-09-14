//! The one door a node's answers enter the run through.
//!
//! A `kind: questions` node's answers are a fact of the run like any
//! other artifact: bytes in the store, an acceptance on the log, and a
//! `questions_answered` that says who answered and through which
//! channel. Every surface that can answer — the console the ask round
//! drives, and the control-plane tool a client calls — reaches the log
//! through this function, so none of them can record a different shape
//! of the same event, and a reply that does not satisfy its questions
//! never touches the log at all.

use std::path::Path;

use yunta_core::events::{ArtifactOrigin, Channel, EventPayload, QuestionsAnsweredPayload};
use yunta_core::{Answer, AnswersFile, ContentHash, NodeId, QuestionsFile, Responder};

use crate::artifacts::{accept, answers_artifact, AcceptError};
use crate::run_log::RunLog;

/// One surface's reply, as the engine records it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub answers: Vec<Answer>,
    pub channel: Channel,
    pub responder: Option<Responder>,
}

/// What the run holds once a reply is recorded.
#[derive(Debug, Clone, PartialEq)]
pub struct Recorded {
    pub answers_hash: ContentHash,
}

/// Why a reply did not become a fact of the run.
#[derive(Debug, thiserror::Error)]
pub enum AnswersError {
    /// The reply does not satisfy the questions it answers, each
    /// violation named. Nothing was written: a surface corrects and
    /// replies again.
    #[error("{}", .violations.join("; "))]
    Refused { violations: Vec<String> },
    #[error("the answers could not be written")]
    Write(#[source] AcceptError),
    #[error("the answers could not be rendered")]
    Render(#[source] yunta_core::yaml::YamlError),
}

impl AnswersError {
    /// The violations a refusal names, for a surface that lists them
    /// rather than printing the whole sentence.
    pub fn violations(&self) -> &[String] {
        match self {
            AnswersError::Refused { violations } => violations,
            _ => &[],
        }
    }
}

/// Records one reply to `node`'s questions: the answers as an artifact
/// of that node, and the `questions_answered` that says how they
/// arrived.
///
/// All or nothing. The reply is judged against the questions it claims
/// to answer *before* anything is written, so a log never holds a
/// half-answered round, and a refusal is a value the caller renders
/// rather than a state the run has to recover from.
pub async fn record(
    log: &RunLog<'_>,
    run_dir: &Path,
    node: &NodeId,
    questions: &QuestionsFile,
    reply: Reply,
) -> Result<Recorded, AnswersError> {
    let violations = yunta_core::validate_answers(questions, &reply.answers);
    if !violations.is_empty() {
        return Err(AnswersError::Refused { violations });
    }
    let file = AnswersFile {
        answers: reply.answers,
    };
    let accepted = write(log, run_dir, node, &file, ArtifactOrigin::Answered).await?;
    log.record(
        Some(node),
        EventPayload::QuestionsAnswered(QuestionsAnsweredPayload {
            answers_hash: accepted.answers_hash.clone(),
            channel: reply.channel,
            responder: reply.responder,
        }),
    )
    .await
    .map_err(|source| {
        AnswersError::Write(AcceptError::Log {
            name: answers_artifact().view_name(),
            source,
        })
    })?;
    Ok(accepted)
}

/// The answers of a node that asked nothing: empty, and the engine's
/// own, so the node after it mounts what it declared to mount whether
/// or not anyone was asked anything.
pub(crate) async fn record_nothing_asked(
    log: &RunLog<'_>,
    run_dir: &Path,
    node: &NodeId,
) -> Result<Recorded, AnswersError> {
    let file = AnswersFile {
        answers: Vec::new(),
    };
    write(log, run_dir, node, &file, ArtifactOrigin::Derived).await
}

async fn write(
    log: &RunLog<'_>,
    run_dir: &Path,
    node: &NodeId,
    file: &AnswersFile,
    origin: ArtifactOrigin,
) -> Result<Recorded, AnswersError> {
    let bytes = yunta_core::yaml::to_string(file)
        .map_err(AnswersError::Render)?
        .into_bytes();
    let accepted = accept(log, run_dir, Some(node), answers_artifact(), &bytes, origin)
        .await
        .map_err(AnswersError::Write)?;
    Ok(Recorded {
        answers_hash: accepted.content_hash,
    })
}
