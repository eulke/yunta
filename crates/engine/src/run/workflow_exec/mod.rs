//! `kind: workflow` — composition as **linked runs**: each
//! sub-workflow is a complete run (own run_id, manifest, event log and
//! run.dir), never an inline expansion. The parent's birth freezes the
//! child's workflow text and prompts with the rest of its inputs, and the
//! child is born from that frozen copy; history pins the child's manifest
//! through the recorded `child_run_id` — reproducing an old parent never
//! re-resolves `name@current`.
//!
//! Mechanics this module fixes:
//! - `use: <name>` is read from the parent run's frozen set, as its
//!   birth froze it: the repo's versioned `.yunta/workflows/<name>.yaml`
//!   catalog first, a publisher's vendored packs second — the same
//!   resolver `list_workflows` and `check_workflow_refs` share. A run
//!   that predates frozen inputs reads the name from its checkout.
//! - The child run id comes from the run's injected `IdSource`, like
//!   every run id; the link to this parent and node is the
//!   `child_run_created` on the parent's log, never the name.
//! - `child_run_created` lands on the parent's log *before* the child's
//!   own `run_created`: a crash in between leaves a dangling reference
//!   (a child with no events) that the next resume supersedes with a
//!   fresh birth under a fresh id, instead of a half-created run
//!   colliding with its own re-creation.
//! - Budgets cascade at birth: the child's frozen
//!   `limits.max_tokens_per_run` is the parent's *remaining* budget, so
//!   the parent's cap bounds the whole tree; the child's spend
//!   aggregates back up through this node's own close.
//! - `isolation: worktree` (default) branches the child's tree off the
//!   HEAD of the tree this node works in; `none` runs the child
//!   directly in that tree — the child never owns (nor cleans up, nor
//!   commits) a tree that isn't its own. One word at every level, so
//!   what the node declares is what the child's manifest freezes.
//!
//! Artifacts cross that boundary in both directions, and by the log at
//! each end. [`mounts`] is the way in: what the child is born holding,
//! read out of the log of whichever run holds it. The way back is this
//! node's own close — a child that reaches `Done` hands over every
//! artifact this node declares, taken from the child's log under
//! `origin: inherited`, so a composition produces what it says it
//! produces without the parent ever reading the child's directory.

mod child;
mod children;
mod hooks;
mod mounts;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use yunta_core::events::{ChildRefusal, Failure};
use yunta_core::{InputName, Isolation, MountSpec, Node};

use crate::frozen::read::Source;

use super::node_close::fail;
use super::node_exec::NodeEnd;
use super::{RunCtx, RunError};

use child::{drive_child, resume_child, Child};

/// Where this parent's runs live — the parent's own run.dir sits inside
/// it, so no configuration lookup can ever disagree with where the
/// parent actually is.
fn runs_root(ctx: &RunCtx<'_>) -> PathBuf {
    ctx.run_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| ctx.run_dir.to_path_buf())
}

/// The worktrees root for child trees: the parent's own.
fn worktrees_root(ctx: &RunCtx<'_>) -> PathBuf {
    crate::run_dir::worktrees_root(ctx.manifest, ctx.run_dir)
}

/// The failure of a composed workflow the run could not start, named by the
/// `use:` that asked for it.
fn child_refused(use_name: &str, refusal: ChildRefusal) -> Failure {
    Failure::ChildRefused {
        workflow: use_name.to_string(),
        refusal: Box::new(refusal),
    }
}

/// What a `kind: workflow` node declares: which workflow to run, what
/// to hand it, and how much of the parent's tree it sees. One value
/// because it is one clause of the node, read together and never apart.
pub(super) struct WorkflowCall<'a> {
    pub use_name: &'a str,
    pub inputs: &'a BTreeMap<InputName, String>,
    pub isolation: Isolation,
    pub mounts: &'a [MountSpec],
}

/// What a step of a `kind: workflow` node decides: carry on with what it made,
/// or end the node the way the step ended it.
pub(super) enum Step<T> {
    Proceed(T),
    Ended(NodeEnd),
}

/// The end of a node that the step refusing it ended: the failure it closes
/// with, or the cut it stops at.
pub(super) fn ended<T>(end: Result<NodeEnd, RunError>) -> Result<Step<T>, RunError> {
    Ok(Step::Ended(end?))
}

pub(super) async fn execute_workflow(
    ctx: &RunCtx<'_>,
    node: &Node,
    call: WorkflowCall<'_>,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    if let Some(failure) = hooks::nested_too_deep(ctx, call.use_name) {
        return fail(ctx, node, failure).await;
    }

    // Resume before create (a parent's resume recursively resumes
    // orphaned children): the last child this node created that
    // never reached child_run_finished — and actually exists (a
    // dangling reference from a crash between the parent's event and
    // the child's run_created has no events, and is superseded below).
    let view = ctx.run_view().await?;
    if let Some(open_child) = view
        .state
        .children
        .open_under(&node.id)
        .map(|link| &link.run_id)
    {
        if !ctx
            .storage
            .events_for_run(open_child.clone())
            .await?
            .is_empty()
        {
            return resume_child(ctx, node, open_child, cancel).await;
        }
    }

    // Fresh birth: the child's text is the one this run reads for its
    // child — the copy its parent froze at birth, or the checkout's file
    // for a run that predates frozen inputs.
    let source = Source::of(ctx.manifest, ctx.run_dir, ctx.worktree, ctx.ambient);
    let admitted = match hooks::admit(ctx, node, &call, &source, &view).await? {
        Step::Proceed(admitted) => admitted,
        Step::Ended(end) => return Ok(end),
    };
    let born = match children::birth(ctx, node, cancel, &call, admitted, &view.state).await? {
        Step::Proceed(born) => born,
        Step::Ended(end) => return Ok(end),
    };
    drive_child(
        ctx,
        node,
        Child {
            id: &born.id,
            manifest: &born.manifest,
            run_dir: &born.run_dir,
            tree: &born.tree,
        },
        cancel,
    )
    .await
}
