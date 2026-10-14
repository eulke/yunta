//! `kind: pull_request` — the run's own branch, pushed and opened as a
//! pull request into the project's base branch through the forge the
//! project configures.
//!
//! Running it again pushes the same branch and finds the pull request it
//! opened by the run's marker, so a rerun never opens a second one.

use chrono::{DateTime, Utc};
use yunta_core::events::{
    AccountLimit, EngineFact, EventPayload, Failure, LimitSource, NodeEvent, NodeOutcome,
    PullRequestOpenedPayload, TokenUsage,
};
use yunta_core::port::{Forge, ForgeError, PullRequestRef, PullRequestRequest};
use yunta_core::{ConfigKey, Node};

use super::node_close::{close_node, fail, fail_with, Close};
use super::node_exec::{render_or_fail, NodeEnd};
use super::step::Step;
use super::{RunCtx, RunError};

pub(super) async fn execute_pull_request(
    ctx: &RunCtx<'_>,
    node: &Node,
    title: &str,
    body: Option<&str>,
    receipt: bool,
) -> Result<NodeEnd, RunError> {
    let (forge, request) = match prepared(ctx, node, title, body, receipt).await? {
        Step::Value(prepared) => prepared,
        Step::Ended(end) => return Ok(end),
    };
    let head = request.head.clone();
    let remote = ctx
        .manifest
        .config
        .forge
        .as_ref()
        .and_then(|forge| forge.github.as_ref())
        .map_or("origin", |github| github.remote());
    if let Err(error) =
        crate::git::push_branch(ctx.worktree, remote, &head, ctx.root_supervision()).await
    {
        if error.cancelled() {
            return Err(error.into());
        }
        let engine_error = format!("pushing `{head}` to `{remote}`: {}", error.detail());
        return fail(ctx, node, Failure::EngineFailed { engine_error }).await;
    }
    let opened = match forge.open_pull_request(&request).await {
        Ok(opened) => opened,
        Err(error) => return open_refused(ctx, node, &error).await,
    };
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::PullRequestOpened(PullRequestOpenedPayload {
            url: opened.url.clone(),
            number: opened.number,
            head,
            base: request.base,
        })),
    )
    .await?;
    let outcome = opened_fact(opened);
    close_node(ctx, node, Close::new(outcome, TokenUsage::default())).await
}

/// The fact a pull request node closes with: the pull request it opened.
fn opened_fact(opened: PullRequestRef) -> NodeOutcome {
    NodeOutcome::Fact(EngineFact::PullRequestOpened {
        number: opened.number,
        url: opened.url,
    })
}

/// Fails the node with the forge's refusal to open the pull request: a spent
/// rate limit is the account's limit, and any other refusal is its sentence.
async fn open_refused(
    ctx: &RunCtx<'_>,
    node: &Node,
    error: &ForgeError,
) -> Result<NodeEnd, RunError> {
    if let Some(failure) = forge_limit(error, ctx.clock.now()) {
        return fail_with(ctx, node, failure, TokenUsage::default()).await;
    }
    let engine_error = format!("opening the pull request: {}", yunta_core::describe(error));
    fail(ctx, node, Failure::EngineFailed { engine_error }).await
}

/// The failure a forge refusal leaves when the forge's rate limit is spent:
/// the account's limit, resetting `retry_after` past `failed_at` when the
/// forge named a wait. Any other refusal is not a limit, so `None`.
pub(super) fn forge_limit(error: &ForgeError, failed_at: DateTime<Utc>) -> Option<Failure> {
    let ForgeError::RateLimited { retry_after, .. } = error else {
        return None;
    };
    let resets_at = retry_after
        .and_then(|wait| chrono::Duration::from_std(wait).ok())
        .map(|wait| failed_at + wait);
    Some(Failure::Limit {
        limit: AccountLimit {
            source: LimitSource::Forge,
            resets_at,
            message: yunta_core::describe(error),
        },
    })
}

/// The forge the pull request is opened through and what it asks for,
/// or the node ended: a key the frozen config leaves unset, a forge this
/// machine cannot reach, a template that does not render.
async fn prepared<'a>(
    ctx: &RunCtx<'a>,
    node: &Node,
    title: &str,
    body: Option<&str>,
    receipt: bool,
) -> Result<Step<(&'a dyn Forge, PullRequestRequest)>, RunError> {
    for key in [ConfigKey::Forge, ConfigKey::RunBranch] {
        if !key.is_declared(&ctx.manifest.config) {
            return Ok(Step::Ended(super::check_exec::unset(ctx, node, key).await?));
        }
    }
    // The config declares a forge; what the run could not build it from
    // is the token, which a person can set before the node runs again.
    let Some(forge) = ctx.forge else {
        let engine_error = "the forge the config declares is not reachable from this \
                            machine: the variable its `token_env` names is not set";
        let failure = Failure::EngineFailed {
            engine_error: engine_error.to_string(),
        };
        return Ok(Step::Ended(fail(ctx, node, failure).await?));
    };
    let title = match render_or_fail(ctx, node, title).await? {
        Step::Value(title) => title,
        Step::Ended(end) => return Ok(Step::Ended(end)),
    };
    let body = match render_or_fail(ctx, node, body.unwrap_or_default()).await? {
        Step::Value(body) => body,
        Step::Ended(end) => return Ok(Step::Ended(end)),
    };
    let request = PullRequestRequest {
        head: crate::worktree::run_branch(ctx.run_id),
        base: base_branch(ctx),
        title,
        body,
        run_id: ctx.run_id.to_string(),
        receipt: match receipt {
            true => Some(receipt_so_far(ctx).await?),
            false => None,
        },
    };
    Ok(Step::Value((forge, request)))
}

/// The run's receipt as its log stands, its hash chain verified: what a
/// pull request opened as the run's last step tells its reviewer.
async fn receipt_so_far(ctx: &RunCtx<'_>) -> Result<yunta_core::receipt::Receipt, RunError> {
    let events = ctx.load_events().await?;
    let chain = match ctx.storage.verify_chain(ctx.run_id.clone()).await? {
        yunta_storage::ChainVerification::Intact { events } => {
            yunta_core::receipt::EventChainStatus::Intact { events }
        }
        yunta_storage::ChainVerification::Broken { seq, detail } => {
            yunta_core::receipt::EventChainStatus::Broken { seq, detail }
        }
    };
    Ok(crate::receipt::receipt_so_far(
        ctx.run_id,
        ctx.manifest,
        &events,
        chain,
    ))
}

/// The branch a pull request goes into: the project's base branch, or
/// the branch the run started from when the project names none.
fn base_branch(ctx: &RunCtx<'_>) -> String {
    ctx.manifest
        .config
        .project
        .as_ref()
        .and_then(|project| project.base_branch.clone())
        .unwrap_or_else(|| ctx.manifest.base_branch.clone())
}
