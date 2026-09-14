//! The `AskQuestions` schedule step: a node the log derives as waiting
//! on the questions it asked gets them put to a person through
//! `HumanInteraction::ask` — the ONE ask site, serving the first
//! invocation (right after the node closed asking) and every later
//! `yunta resume` through the identical path, with zero conversational
//! state: the questions are re-read from the artifact the run holds,
//! never from memory.
//!
//! The round records an answer and nothing else. The node's own close
//! already ran — its hooks, its scope audit, its artifacts — when it
//! asked, so there is no session to open, no attempt to count and no
//! second close to run here. What the round owes the node is its
//! terminal, and `FinishAnswered` pays it from the log.

use yunta_core::events::{ArtifactId, EventPayload, PauseReason, QuestionsAskedPayload};
use yunta_core::NonEmpty;
use yunta_core::{ArtifactKind, Node};

use crate::answers::{AnswersError, Reply};
use crate::artifacts::{describe, RunArtifacts};

use super::{RunCtx, RunError};
use yunta_core::events::GateEvent;

/// What the ask round produced: the answers are on the log, or a reason
/// to pause — no surface, or a reply the questions refuse, each citing
/// exactly what is missing.
pub(super) enum AskOutcome {
    Answered,
    Pause { reason: PauseReason },
}

#[tracing::instrument(skip_all, fields(run_id = %ctx.run_id, node_id = %node.id))]
pub(super) async fn execute_ask(ctx: &RunCtx<'_>, node: &Node) -> Result<AskOutcome, RunError> {
    let events = ctx.load_events().await?;
    let asked = pending(&events, node).ok_or_else(|| RunError::Broken {
        diagnostic: format!(
            "node `{}` is waiting on questions the log does not name — no `questions_asked` \
             stands for it, so there is nothing to ask",
            node.id
        ),
    })?;

    // The document the node asked from, as the run holds it: the
    // acceptance on the log and the bytes in the store are what the
    // round re-reads, so no file could have changed underneath it. The
    // fact names the hash it asked from, and a run that holds different
    // bytes under that identity is a run whose round no longer means
    // what it said.
    let held = RunArtifacts::of(ctx.run_dir, &events);
    let questions = ArtifactId::Interpreted {
        kind: ArtifactKind::Questions,
    };
    let found = held
        .held(&questions, Some(&node.id))
        .ok_or_else(|| RunError::Broken {
            diagnostic: format!(
                "node `{}` asked from a questions document the run no longer holds",
                node.id
            ),
        })?;
    if found.content_hash != asked.questions_hash {
        return Err(RunError::Broken {
            diagnostic: format!(
                "node `{}` asked from `{}`, and the questions the run holds are `{}` — the \
                 round would answer a different document than the one it asked",
                node.id, asked.questions_hash, found.content_hash
            ),
        });
    }
    let bytes = held.bytes(found).await.map_err(|source| RunError::Broken {
        diagnostic: format!("cannot read the questions of node `{}`: {source}", node.id),
    })?;
    // The same door `close_artifacts` reads a questions file through:
    // the round names what is wrong with the document, never what a
    // deserializer made of it.
    let file = yunta_core::shape::read::<yunta_core::QuestionsFile>(&bytes, describe(found))?;

    let Some(reply) = ctx.human_interaction.ask(&file).await else {
        // No surface can answer right now (headless, `yunta test`, a
        // piped invocation): the run parks, and the questions stand for
        // whichever surface reaches them next.
        return Ok(AskOutcome::Pause {
            reason: PauseReason::Questions {
                node: node.id.clone(),
                pending: NonEmpty::new(asked.questions.clone()).ok_or_else(|| {
                    RunError::Broken {
                        diagnostic: format!(
                            "node `{}` recorded that it asked and named no question",
                            node.id
                        ),
                    }
                })?,
            },
        });
    };

    match crate::answers::record(
        &ctx.log(),
        ctx.run_dir,
        &node.id,
        &file,
        Reply {
            answers: reply.answers,
            channel: reply.channel,
            responder: reply.responder,
        },
    )
    .await
    {
        Ok(_) => Ok(AskOutcome::Answered),
        // The surface answered and the questions refused the reply. The
        // engine is the verdict-giver, so nothing is half-recorded: the
        // run parks citing the exact violations, and the next round asks
        // again from the same document.
        Err(error @ AnswersError::Refused { .. }) => Ok(AskOutcome::Pause {
            reason: PauseReason::AnswersRefused {
                node: node.id.clone(),
                violations: error.violations().to_vec(),
            },
        }),
        Err(other) => Err(RunError::Broken {
            diagnostic: other.to_string(),
        }),
    }
}

/// The questions this node asked and nobody has answered: its latest
/// `questions_asked` with no `questions_answered` after it.
fn pending(
    events: &[yunta_core::events::StoredEvent],
    node: &Node,
) -> Option<QuestionsAskedPayload> {
    let mut asked = None;
    for event in events {
        if event.node_id.as_ref() != Some(&node.id) {
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
