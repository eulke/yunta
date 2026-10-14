//! What a node's sessions may do beside writing their scope: the agent
//! they work as, and whose configuration of the agent's CLI they load.
//! Each is the node's own word, else the workflow's, else a default.
//!
//! What a node reads from the run's frozen inputs is read through
//! [`Source`], and a refusal of one closes the node the same way wherever
//! it is read: see [`close_frozen`].

use yunta_core::agent::AgentDefinition;
use yunta_core::events::{Failure, FrozenRead, TokenUsage};
use yunta_core::Node;

use super::node_exec::NodeEnd;
use crate::frozen::{FrozenError, Source};

/// Whose CLI configuration `node`'s sessions load: its own word, else the
/// workflow's `node_defaults`, else the person's own.
pub(super) fn cli_config(ctx: &crate::run::RunCtx<'_>, node: &Node) -> yunta_core::CliConfig {
    node.cli_config
        .or_else(|| {
            ctx.manifest
                .workflow
                .node_defaults
                .as_ref()
                .and_then(|defaults| defaults.cli_config)
        })
        .unwrap_or_default()
}

/// The definition of the agent the node runs as, if it names one, as the
/// run froze it at birth. A name that resolves to no definition fails the
/// node before any session spends a token on it.
pub(super) async fn agent(
    ctx: &crate::run::RunCtx<'_>,
    node: &Node,
    chosen: &yunta_core::RunnerCandidate,
) -> Result<Result<Option<AgentDefinition>, NodeEnd>, crate::run::RunError> {
    let Some(name) = &chosen.agent else {
        return Ok(Ok(None));
    };
    match Source::of(ctx.manifest, ctx.run_dir, ctx.worktree, ctx.ambient)
        .agent(name)
        .await
    {
        Ok(agent) => Ok(Ok(Some(agent))),
        Err(error) => Ok(Err(close_frozen(ctx, node, error).await?)),
    }
}

/// Closes `node` with a refusal of one of the run's frozen inputs. Each
/// refusal comes from one place:
///
/// - `Missing` and `Mismatch` come from a frozen set: a name the set does not
///   hold, an object the run's store does not hold, a copy that is gone or
///   whose files differ from the set, or bytes that no longer hash to the
///   value the run was born with. Both close the node as `FrozenInput`,
///   naming the input.
/// - `Unresolved` comes from a checkout read, which a run without a frozen
///   set makes, and from a frozen agent whose bytes do not read as a
///   definition. `Escapes` comes only from the freeze at birth, which no node
///   read reaches, and `Prompt` from a prompt file a node reads. All three
///   close the node as `FrozenRead`, which says what the read could not give.
/// - `UnreadableKey` comes from a frozen set that holds a knowledge key no
///   layer reads: an entry this binary cannot place, not a changed input.
/// - `Io` comes from either. Both are steps of the engine that failed on their
///   own, and close the node as `EngineFailed` with the error's own sentence.
pub(super) async fn close_frozen(
    ctx: &crate::run::RunCtx<'_>,
    node: &Node,
    error: FrozenError,
) -> Result<NodeEnd, crate::run::RunError> {
    let failure = match error {
        FrozenError::Mismatch { input } | FrozenError::Missing { input } => Failure::FrozenInput {
            frozen_input: input,
        },
        FrozenError::Unresolved(problem) => Failure::FrozenRead {
            frozen_read: FrozenRead::Unresolved { problem },
        },
        FrozenError::Escapes { pack, path } => Failure::FrozenRead {
            frozen_read: FrozenRead::Escapes { pack, path },
        },
        FrozenError::Prompt {
            node: prompt_node,
            path,
            ..
        } => Failure::FrozenRead {
            frozen_read: FrozenRead::Prompt {
                node: prompt_node,
                path,
            },
        },
        other @ (FrozenError::UnreadableKey { .. } | FrozenError::Io { .. }) => {
            Failure::EngineFailed {
                engine_error: other.to_string(),
            }
        }
    };
    super::node_close::fail_with(ctx, node, failure, TokenUsage::default()).await
}
