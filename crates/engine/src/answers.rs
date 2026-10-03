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

use yunta_core::diagnostic::Report;
use yunta_core::events::{
    ArtifactId, Channel, EventPayload, QuestionsAnsweredPayload, QuestionsAskedPayload,
    RecordedOrigin, StoredEvent,
};
use yunta_core::{
    Answer, AnswersFile, ArtifactKind, Clock, ContentHash, Manifest, NodeId, QuestionsFile,
    Responder, RunId,
};

use crate::artifacts::{accept, AcceptError, RunArtifacts};
use crate::run_log::RunLog;
use yunta_core::events::GateEvent;

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
    /// The reply does not answer the questions it claims to, every way
    /// it fails named. Nothing was written: a surface corrects and
    /// replies again.
    #[error("{0}")]
    Refused(Report),
    #[error("the answers could not be written")]
    Write(#[source] AcceptError),
    #[error("the answers could not be rendered")]
    Render(#[source] yunta_core::yaml::YamlError),
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
    let file = AnswersFile::against(questions, reply.answers).map_err(AnswersError::Refused)?;
    let accepted = write(log, run_dir, node, &file, RecordedOrigin::Answered).await?;
    log.record(
        Some(node),
        EventPayload::Gates(GateEvent::QuestionsAnswered(QuestionsAnsweredPayload {
            answers_hash: accepted.answers_hash.clone(),
            channel: reply.channel,
            responder: reply.responder,
        })),
    )
    .await
    .map_err(|source| {
        AnswersError::Write(AcceptError::Log {
            name: ArtifactId::Interpreted {
                kind: ArtifactKind::Answers,
            }
            .view_name(),
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
    write(log, run_dir, node, &file, RecordedOrigin::Derived).await
}

async fn write(
    log: &RunLog<'_>,
    run_dir: &Path,
    node: &NodeId,
    file: &AnswersFile,
    origin: RecordedOrigin,
) -> Result<Recorded, AnswersError> {
    let bytes = yunta_core::yaml::to_string(file)
        .map_err(AnswersError::Render)?
        .into_bytes();
    let accepted = accept(
        log,
        run_dir,
        Some(node),
        ArtifactId::Interpreted {
            kind: ArtifactKind::Answers,
        },
        &bytes,
        origin,
    )
    .await
    .map_err(AnswersError::Write)?;
    Ok(Recorded {
        answers_hash: accepted.content_hash,
    })
}

/// Why a reply from outside the run did not reach its log.
#[derive(Debug, thiserror::Error)]
pub enum AnswerQuestionsError {
    /// The node has no open round. Either it never asked, or its
    /// questions are already answered — and a surface answering a round
    /// that closed would be recording a second reply to it.
    #[error(
        "node `{node}` is not waiting on questions — it asked none, or the ones it asked are \
         already answered"
    )]
    NothingAsked { node: NodeId },
    /// The run no longer holds the document the round asked from, or
    /// holds different bytes under that identity. A reply then answers
    /// a different document than the one that was asked.
    #[error("{0}")]
    Unreadable(String),
    #[error(transparent)]
    Answers(#[from] AnswersError),
    #[error(transparent)]
    Storage(#[from] yunta_storage::StorageError),
}

/// Records a reply to `node`'s open questions from a process that never
/// ran the node — the control plane's own surface on this door, as
/// `resolve_gate` is the control plane's surface on a gate decision.
///
/// It reads the round the same way the console round reads it, judges
/// the reply against the very document that was asked from, and writes
/// the acceptance and the `questions_answered` or nothing at all. What
/// finishes the node is a later `resume`: the engine's own
/// `FinishAnswered` step consumes the answer, so a reply recorded here
/// and a reply typed at a console leave the run in the same state.
pub async fn answer_questions(
    manifest: &Manifest,
    storage: &yunta_storage::AsyncStorage,
    run_id: &RunId,
    run_dir: &Path,
    clock: &dyn Clock,
    node: &NodeId,
    reply: Reply,
) -> Result<Recorded, AnswerQuestionsError> {
    let events = storage.events_for_run(run_id.clone()).await?;
    let (_, questions) = asked_from(run_dir, &events, node).await?;
    // Nothing reads a secret back out of an answer a person typed, so
    // the same redactor the run itself appends through stands between
    // this reply and the log.
    let redactor = yunta_core::Redactor::of(&manifest.config.secrets, None);
    let log = RunLog::new(storage, run_id, clock, &redactor);
    Ok(record(&log, run_dir, node, &questions, reply).await?)
}

/// The round `node` is waiting on and the document it asked from, as the
/// run holds it.
///
/// One reading for both surfaces. The acceptance on the log and the
/// bytes in the store are what a round re-reads, checked against the
/// hash its `questions_asked` named: a run holding different bytes
/// under that identity is a run whose round no longer means what it
/// said, and answering it would answer a different document.
pub(crate) async fn asked_from(
    run_dir: &Path,
    events: &[StoredEvent],
    node: &NodeId,
) -> Result<(QuestionsAskedPayload, QuestionsFile), AnswerQuestionsError> {
    let asked = open_round(events, node)
        .ok_or_else(|| AnswerQuestionsError::NothingAsked { node: node.clone() })?;
    let held = RunArtifacts::of(run_dir, events);
    let identity = ArtifactId::Interpreted {
        kind: ArtifactKind::Questions,
    };
    let found = held.held(&identity, Some(node)).ok_or_else(|| {
        AnswerQuestionsError::Unreadable(format!(
            "node `{node}` asked from a questions document the run no longer holds"
        ))
    })?;
    if found.content_hash != asked.questions_hash {
        return Err(AnswerQuestionsError::Unreadable(format!(
            "node `{node}` asked from `{}`, and the questions the run holds are `{}` — the \
             round would answer a different document than the one it asked",
            asked.questions_hash, found.content_hash
        )));
    }
    let bytes = held.bytes(found).await.map_err(|source| {
        AnswerQuestionsError::Unreadable(format!(
            "cannot read the questions of node `{node}`: {source}"
        ))
    })?;
    let file = yunta_core::shape::read::<QuestionsFile>(&bytes, crate::artifacts::describe(found))
        .map_err(|report| AnswerQuestionsError::Answers(AnswersError::Refused(report)))?;
    Ok((asked, file))
}

/// The questions `node` asked and nobody has answered: its latest
/// `questions_asked` with no `questions_answered` after it.
fn open_round(events: &[StoredEvent], node: &NodeId) -> Option<QuestionsAskedPayload> {
    let mut asked = None;
    for event in events {
        if event.node_id.as_ref() != Some(node) {
            continue;
        }
        match event.payload() {
            Some(EventPayload::Gates(GateEvent::QuestionsAsked(p))) => asked = Some(p.clone()),
            Some(EventPayload::Gates(GateEvent::QuestionsAnswered(_))) => asked = None,
            _ => {}
        }
    }
    asked
}
