//! A task's tests written again.
//!
//! When a person accepts that a test the run's spec gave a task is
//! wrong, the node that wrote the spec writes that task's tests again —
//! nobody who builds a task writes the test that judges it. The loop
//! sends the run back to that node before it dispatches anything else,
//! and goes on once the node has handed a spec over again: every node
//! between them runs again in order, so a gate that approved the tests
//! asks about the new ones.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use yunta_core::events::artifacts::ArtifactLedger;
use yunta_core::events::{
    ArtifactId, EventPayload, Failure, NodeEvent, NodeReroutedPayload, RerouteCause, RerouteOrigin,
    StoredEvent, TokenUsage,
};
use yunta_core::{ArtifactKind, Node, NodeId, NodeKind, SpecFile, TaskId};

use super::super::node_close::fail_with_tokens;
use super::super::node_exec::NodeEnd;
use super::super::{RunCtx, RunError};
use crate::replay::RunState;

/// The node that writes the run's spec again: the one the log accepted
/// the spec it holds from, when that node opens a session of its own and
/// the run's mode keeps it. `None` when the spec came with the run.
pub(super) fn writer(ctx: &RunCtx<'_>, events: &[StoredEvent], state: &RunState) -> Option<NodeId> {
    let producer = state
        .artifacts
        .of_kind(ArtifactKind::Spec)
        .last()?
        .producer
        .clone()?;
    let workflow = &ctx.manifest.workflow;
    let mode = yunta_core::events::run_mode(events);
    let left_out = yunta_core::events::run_left_out(events);
    let kept = crate::modes::included_nodes(workflow, &mode, &left_out)
        .is_none_or(|ids| ids.contains(&producer));
    let writes = workflow
        .iter_nodes()
        .any(|node| node.id == producer && matches!(node.kind, NodeKind::Prompt { .. }));
    (kept && writes).then_some(producer)
}

/// Sends the run back to the node that writes the spec when a person
/// accepted that a task's tests are wrong and that node has not written
/// them again: the loop fails, retryable, and re-routes there, as a
/// person's choice does — it spends none of the loop's own re-routes.
pub(super) async fn send_back(
    ctx: &RunCtx<'_>,
    node: &Node,
    tokens: TokenUsage,
) -> Result<Option<NodeEnd>, RunError> {
    let state = ctx.run_view().await?.state;
    let Some((task, by)) = crate::tasks::respecifications_owed(&state)
        .into_iter()
        .next()
        .map(|(task, owed)| (task.clone(), owed.by.clone()))
    else {
        return Ok(None);
    };
    let reason = format!(
        "a person accepted that task `{task}`'s tests are wrong — `{by}` writes them again \
         before the loop goes on"
    );
    let end = fail_with_tokens(ctx, node, reason.clone(), true, tokens).await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::Rerouted(NodeReroutedPayload::new(
            by,
            RerouteCause(Failure::message(reason)),
            RerouteOrigin::GateChoice,
            None,
            None,
        ))),
    )
    .await?;
    Ok(Some(end))
}

/// Every file a spec the run accepted before gave a task, which the spec
/// it holds now no longer gives it — what a task picking up work it did
/// under an earlier spec leaves out of that work.
pub(super) async fn superseded(
    run_dir: &Path,
    events: &[StoredEvent],
    current: Option<&SpecFile>,
) -> Result<BTreeMap<TaskId, Vec<PathBuf>>, RunError> {
    let spec = ArtifactId::Interpreted {
        kind: ArtifactKind::Spec,
    };
    let held = crate::artifacts::RunArtifacts::of(run_dir, events);
    let mut ledger = ArtifactLedger::default();
    let mut gone: BTreeMap<TaskId, Vec<PathBuf>> = BTreeMap::new();
    for event in events {
        let Some(EventPayload::Artifacts(artifact)) = event.payload() else {
            continue;
        };
        let Some(accepted) = ledger
            .apply(event.node_id.as_ref(), event.seq, artifact)
            .filter(|accepted| accepted.artifact == spec)
            .cloned()
        else {
            continue;
        };
        let bytes = held
            .bytes(&accepted)
            .await
            .map_err(crate::artifacts::HeldError::from)?;
        let earlier = yunta_core::shape::read::<SpecFile>(&bytes, "an earlier spec".to_string())
            .map_err(crate::artifacts::HeldError::from)?;
        for given in earlier.specs {
            let now: Vec<String> = current
                .and_then(|current| current.of(&given.task))
                .map(|spec| spec.files.iter().map(|file| file.in_repo()).collect())
                .unwrap_or_default();
            let left = gone.entry(given.task.clone()).or_default();
            for file in given.files {
                let path = PathBuf::from(file.in_repo());
                if !now.contains(&file.in_repo()) && !left.contains(&path) {
                    left.push(path);
                }
            }
        }
    }
    gone.retain(|_, paths| !paths.is_empty());
    Ok(gone)
}
