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

use yunta_core::events::PauseReason;
use yunta_core::Node;
use yunta_core::NonEmpty;

use crate::answers::{AnswersError, Reply};

use super::{RunCtx, RunError};

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
    // The round and the document it asked from, read the one way both
    // surfaces read them — this one and the control plane's
    // `answer_questions`, so neither can answer a document the other
    // would have refused.
    let (asked, file) = crate::answers::asked_from(ctx.run_dir, &events, &node.id)
        .await
        .map_err(|source| RunError::Broken {
            diagnostic: source.to_string(),
        })?;

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
        Err(AnswersError::Refused(report)) => Ok(AskOutcome::Pause {
            reason: PauseReason::AnswersRefused {
                node: node.id.clone(),
                report,
            },
        }),
        Err(other) => Err(RunError::Broken {
            diagnostic: other.to_string(),
        }),
    }
}
