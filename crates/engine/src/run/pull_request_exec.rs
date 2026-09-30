//! `kind: pull_request` — the run's own branch, pushed and opened as a
//! pull request into the project's base branch through the forge the
//! project configures.
//!
//! Running it again pushes the same branch and finds the pull request it
//! opened by the run's marker, so a rerun never opens a second one.

use yunta_core::events::{EventPayload, NodeEvent, PullRequestOpenedPayload, TokenUsage};
use yunta_core::port::{Forge, PullRequestRequest};
use yunta_core::{ConfigKey, Node};

use super::node_close::{close_node, fail, Close};
use super::node_exec::{render_or_fail, NodeEnd};
use super::step::Step;
use super::{RunCtx, RunError};

pub(super) async fn execute_pull_request(
    ctx: &RunCtx<'_>,
    node: &Node,
    title: &str,
    body: Option<&str>,
) -> Result<NodeEnd, RunError> {
    let (forge, request) = match prepared(ctx, node, title, body).await? {
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
        let said = format!("pushing `{head}` to `{remote}`: {}", error.detail());
        return fail(ctx, node, said, false).await;
    }
    let opened = match forge.open_pull_request(&request).await {
        Ok(opened) => opened,
        Err(error) => {
            let said = format!("opening the pull request: {}", yunta_core::describe(&error));
            return fail(ctx, node, said, false).await;
        }
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
    let outcome = format!("pull request #{} — {}", opened.number, opened.url);
    close_node(ctx, node, Close::new(outcome, TokenUsage::default())).await
}

/// The forge the pull request is opened through and what it asks for,
/// or the node ended: a key the frozen config leaves unset, a forge this
/// machine cannot reach, a template that does not render.
async fn prepared<'a>(
    ctx: &RunCtx<'a>,
    node: &Node,
    title: &str,
    body: Option<&str>,
) -> Result<Step<(&'a dyn Forge, PullRequestRequest)>, RunError> {
    for key in [ConfigKey::Forge, ConfigKey::RunBranch] {
        if !key.is_declared(&ctx.manifest.config) {
            return Ok(Step::Ended(super::check_exec::unset(ctx, node, key).await?));
        }
    }
    // The config declares a forge; what the run could not build it from
    // is the token, which a person can set before the node runs again.
    let Some(forge) = ctx.forge else {
        let said = "the forge the config declares is not reachable from this machine: the \
                    variable its `token_env` names is not set";
        return Ok(Step::Ended(fail(ctx, node, said.to_string(), false).await?));
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
    };
    Ok(Step::Value((forge, request)))
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
