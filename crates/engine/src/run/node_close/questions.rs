//! The questions round of a node that asked: the questions it handed over are
//! registered on the log, and the round either stops for a person or goes on
//! with what the work assumes without an answer.

use std::path::PathBuf;

use yunta_core::events::{
    EngineFact, EventPayload, GateEvent, NodeOutcome, QuestionsAskedPayload, TokenUsage,
};
use yunta_core::Node;

use super::{finish_node_staged, held_questions};
use crate::run::node_artifacts::Asked;
use crate::run::node_exec::NodeEnd;
use crate::run::{RunCtx, RunError};

/// The questions round of a node that asked. `close_outcome` is the outcome
/// the close would finish with, which the round keeps when no question was
/// asked at all.
pub(super) async fn ask_round(
    ctx: &RunCtx<'_>,
    node: &Node,
    asked: Asked,
    close_outcome: NodeOutcome,
    tokens: TokenUsage,
    staged: &[PathBuf],
) -> Result<NodeEnd, RunError> {
    // The hash the fact names is the one the run's own store
    // answers for, read back off the log rather than taken from
    // the bytes the close happened to hold: the round that
    // follows resolves the same acceptance, and one number that
    // came from two places is one that can disagree with itself.
    let crate::replay::RunView { events, state } = ctx.run_view().await?;
    let questions_hash = held_questions(ctx, &events, &node.id)?;
    // Whether the round waits is the workflow's to say, under the
    // mode the run was created in.
    let waits = asked.waits(ctx.manifest.workflow.ask(node, state.run.mode()));
    match QuestionsAskedPayload::new(questions_hash, asked.ids, tokens) {
        Some(payload) => {
            ctx.emit(
                Some(&node.id),
                EventPayload::Gates(GateEvent::QuestionsAsked(payload)),
            )
            .await?;
            if waits {
                return Ok(NodeEnd::Asked);
            }
            // The workflow lets a round with nothing required go: each question
            // says what the work assumes without an answer, and the run goes on
            // with that rather than waiting for a person.
            crate::answers::record_assumed(&ctx.log(), ctx.run_dir, &node.id)
                .await
                .map_err(|source| RunError::Broken {
                    diagnostic: source.to_string(),
                })?;
            let outcome = NodeOutcome::Fact(EngineFact::QuestionsAssumed);
            let spent = TokenUsage::default();
            finish_node_staged(ctx, node, outcome, spent, staged).await
        }
        // A questions document with no questions asked nothing.
        // Its answers still exist, empty and the engine's own,
        // so the node after it mounts what it declared to mount.
        None => {
            crate::answers::record_nothing_asked(&ctx.log(), ctx.run_dir, &node.id)
                .await
                .map_err(|source| RunError::Broken {
                    diagnostic: source.to_string(),
                })?;
            finish_node_staged(ctx, node, close_outcome, tokens, staged).await
        }
    }
}
