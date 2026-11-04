//! What a gate puts before a reviewer, and the record of where it went.
//!
//! A gate's pull request is assembled here from the run: the artifacts it
//! holds for review, the documents drawn from them, and the decision the
//! pull request asks. A plan that cannot be proven as it is written is sent
//! back instead, as [`flawed`](super::flawed) does. Once the forge holds the
//! pull request, the gate's waiting event names it, the assignee it was
//! addressed to and the folder its files are in, so the next publication goes
//! there too.

use yunta_core::events::{
    Claim, Escalation, EventPayload, Failure, GateEvent, PublishedGate, Shown,
};
use yunta_core::port::{GateDecision, PublishRequest, PullRequestRef};
use yunta_core::shown::ShownDocument;
use yunta_core::{ExternalGate, Node};

use super::gate_exec::{emit_started, encode_ref, GateStep};
use super::node_close::fail;
use super::{RunCtx, RunError};

/// What assembling a gate's publication came to.
pub(super) enum Assembly {
    /// The request the forge publishes. Boxed, so the variant is a pointer and
    /// the request does not set the size of the enum.
    Ready(Box<PublishRequest>),
    /// The step the gate ended on before it published anything.
    Ended(GateStep),
}

/// Assembles what `node`'s gate publishes from the run: the artifacts it holds
/// for review, the documents drawn from them, and the request the forge
/// receives. Nothing is published when the node ends first: an artifact the
/// run does not hold fails it, and a plan that cannot be proven is sent back.
pub(super) async fn assemble(
    ctx: &RunCtx<'_>,
    node: &Node,
    external: &ExternalGate,
    assignee: &str,
    message: Option<&str>,
    branch: String,
) -> Result<Assembly, RunError> {
    let Some((artifacts, shows)) = held_for_review(ctx, node, external).await? else {
        return Ok(Assembly::Ended(GateStep::Resolved));
    };
    let state = ctx.run_view().await?.state;
    let shown =
        crate::artifacts::shown::documents(ctx.run_dir, &shows, &state, ctx.manifest.request())
            .await?;
    let flaws = super::flawed::unprovable(&shown);
    if !flaws.is_empty() {
        let step = super::flawed::send_back(ctx, node, &flaws).await?;
        return Ok(Assembly::Ended(step));
    }
    let request = PublishRequest {
        branch,
        base_branch: ctx.manifest.base_branch.clone(),
        run_id: ctx.run_id.clone(),
        folder: state.gates.folder(ctx.run_id, &node.id),
        decision: decision(ctx, node, assignee, message, &shown),
        artifacts,
        shown,
        changed: state.current_change().cloned(),
    };
    Ok(Assembly::Ready(Box::new(request)))
}

/// What the reviewer decides on, as the run holds it: each artifact's
/// bytes under the name its identity gives it, and the log's name for
/// each. `None` once the node failed for one the run does not hold — a
/// partial review is never published.
async fn held_for_review(
    ctx: &RunCtx<'_>,
    node: &Node,
    external: &ExternalGate,
) -> Result<Option<(Vec<(String, Vec<u8>)>, Vec<Shown>)>, RunError> {
    let events = ctx.load_events().await?;
    let held = crate::artifacts::RunArtifacts::of(ctx.run_dir, &events);
    let mut artifacts = Vec::new();
    let mut shows = Vec::new();
    for spec in &external.artifacts {
        let wanted = yunta_core::events::ArtifactId::from(spec);
        let Some(artifact) = held.held(&wanted, None) else {
            emit_started(ctx, node).await?;
            let failure = Failure::GateArtifactMissing {
                node: node.id.clone(),
                gate_artifact: wanted,
            };
            fail(ctx, node, failure).await?;
            return Ok(None);
        };
        let bytes = held
            .bytes(artifact)
            .await
            .map_err(crate::artifacts::HeldError::from)?;
        artifacts.push((wanted.view_name(), bytes));
        shows.push(Shown {
            producer: artifact.producer.clone(),
            artifact: artifact.artifact.clone(),
            content_hash: artifact.content_hash.clone(),
        });
    }
    Ok(Some((artifacts, shows)))
}

/// What the gate's pull request asks, and what each answer a review can
/// give does to the run: the nodes that wait on it go on once it passes,
/// and a request for changes goes where its `on_failure` sends it.
fn decision(
    ctx: &RunCtx<'_>,
    node: &Node,
    assignee: &str,
    message: Option<&str>,
    shown: &[ShownDocument],
) -> GateDecision {
    GateDecision {
        node: node.id.clone(),
        question: super::internal_gate::claim(node, message, shown),
        assignee: assignee.to_string(),
        then: ctx
            .manifest
            .workflow
            .dependents(&node.id)
            .cloned()
            .collect(),
        corrected_by: node.on_failure.as_ref().map(|on| on.goto.clone()),
    }
}

/// Emits the gate's waiting event for the pull request the forge published
/// from `request`. It names that pull request, the assignee `request` is
/// addressed to and the folder its files went to. The log names no assignee
/// or folder but those the pull request was published with, so the next
/// publication goes to the same folder.
pub(super) async fn emit_waiting(
    ctx: &RunCtx<'_>,
    node: &Node,
    request: &PublishRequest,
    published: &PullRequestRef,
) -> Result<(), RunError> {
    let summary = Claim::AwaitsReview {
        node: node.id.clone(),
        assignee: Some(request.decision.assignee.clone()),
    };
    let handle = encode_ref(published)?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Waiting(Box::new(
            Escalation::published_to(
                summary,
                PublishedGate {
                    url: published.url.clone(),
                    folder: request.folder.recorded().map(str::to_string),
                },
                handle,
            )
            .into_payload(),
        ))),
    )
    .await?;
    Ok(())
}
