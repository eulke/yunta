//! A `kind: gate` answered here rather than on a forge: the question it
//! puts, the documents it shows the person deciding, and where their
//! answer sends the run.

use indexmap::IndexMap;
use yunta_core::events::artifacts::ArtifactLedger;
use yunta_core::events::{
    ArtifactId, Escalation, EscalationError, EventPayload, Fact, GateEvent, GateResolvedPayload,
    HumanChoice, NodeEvent, PauseReason, Shown, TokenUsage,
};
use yunta_core::{ArtifactContextRef, Node, NodeId, NodeKind, NonEmpty, OptionId, Workflow};

use super::gate_exec::{emit_started, GateStep};
use super::node_close::{fail, finish_node};
use super::{RunCtx, RunError};
use crate::replay::RunState;
use crate::reserved::{offers, ReservedOption};

/// A gate a person answers here, read once out of its node.
pub(crate) struct InternalGate<'a> {
    node: &'a Node,
    assignee: &'a str,
    message: Option<&'a str>,
    options: &'a [OptionId],
    on: &'a IndexMap<OptionId, NodeId>,
    shows: &'a [ArtifactContextRef],
}

/// Why a gate's escalation could not be put together.
#[derive(Debug, thiserror::Error)]
pub(crate) enum GateEscalationError {
    #[error(transparent)]
    Escalation(#[from] EscalationError),
    /// What it shows is a read the gate waited for, so a run without it
    /// has a log that says otherwise.
    #[error("it shows the {artifact}{of}, which the run does not hold")]
    NotHeld { artifact: ArtifactId, of: String },
    #[error("the run's findings it shows could not be written: {0}")]
    View(#[from] yunta_core::yaml::YamlError),
}

impl<'a> InternalGate<'a> {
    /// `node` as a gate answered here; `None` for any other node.
    pub(crate) fn of(node: &'a Node) -> Option<Self> {
        let NodeKind::Gate {
            assignee,
            message,
            options,
            on,
            shows,
            external: None,
        } = &node.kind
        else {
            return None;
        };
        Some(InternalGate {
            node,
            assignee,
            message: message.as_deref(),
            options,
            on,
            shows,
        })
    }

    /// What the gate asks: its declared options (default: a single
    /// `approve`), each saying where it sends the run — and asking what
    /// should change when it sends the run back to a session — plus
    /// `abort`, and the documents it shows as the run holds them now.
    ///
    /// `abort` is always the engine's: declaring it only places it on the
    /// menu, and choosing it pauses the run wherever it sits.
    ///
    /// The one builder of it: a `resolve_gate` call from a process that
    /// never paused this run rebuilds the identical object from the log.
    pub(crate) fn escalation(
        &self,
        workflow: &Workflow,
        state: &RunState,
    ) -> Result<Escalation, GateEscalationError> {
        let declared: Vec<OptionId> = if self.options.is_empty() {
            vec![ReservedOption::Approve.id()]
        } else {
            self.options.to_vec()
        };
        let mut gate_options: Vec<_> = declared
            .iter()
            .map(|id| {
                if aborts(id) {
                    return offers::abort();
                }
                let target = self.on.get(id).and_then(|target| {
                    workflow
                        .iter_nodes()
                        .find(|candidate| candidate.id == *target)
                });
                offers::declared(id, target)
            })
            .collect();
        if !declared.iter().any(aborts) {
            gate_options.push(offers::abort());
        }
        let (first, rest) = gate_options
            .split_first()
            .map(|(first, rest)| (first.clone(), rest.to_vec()))
            .unwrap_or_else(|| (offers::abort(), Vec::new()));
        // An author's own `message:` is the claim; the assignee is the
        // record of who it is addressed to.
        Ok(Escalation::new(
            self.message
                .map(str::to_string)
                .unwrap_or_else(|| format!("gate `{}` needs a decision", self.node.id)),
            vec![Fact::labelled("assignee", self.assignee)].into(),
            NonEmpty::from((first, rest)),
        )?
        .showing(self.shown(state)?))
    }

    /// Whether the gate shows the run's findings.
    fn shows_the_run_findings(&self) -> bool {
        self.shows.iter().any(super::gate_findings::is_view)
    }

    /// Each document the gate shows, as the run holds it now; the run's
    /// findings as the view the log derives.
    fn shown(&self, state: &RunState) -> Result<Vec<Shown>, GateEscalationError> {
        let artifacts: &ArtifactLedger = &state.artifacts;
        self.shows
            .iter()
            .map(|reference| {
                if super::gate_findings::is_view(reference) {
                    return Ok(super::gate_findings::shown(state)?);
                }
                let artifact = ArtifactId::from(&reference.id);
                let held = artifacts
                    .latest(&artifact, reference.node.as_ref())
                    .ok_or_else(|| GateEscalationError::NotHeld {
                        artifact: artifact.clone(),
                        of: reference
                            .node
                            .as_ref()
                            .map(|node| format!(" of `{node}`"))
                            .unwrap_or_default(),
                    })?;
                Ok(Shown {
                    producer: held.producer.clone(),
                    artifact: held.artifact.clone(),
                    content_hash: held.content_hash.clone(),
                })
            })
            .collect()
    }
}

/// Whether choosing `option` pauses the run.
fn aborts(option: &OptionId) -> bool {
    ReservedOption::of(option) == Some(ReservedOption::Abort)
}

/// Resolves an internal gate: puts its escalation to the person, or
/// takes the decision `resolve_gate` seeded onto the log while the run
/// was parked. An option mapped in `on` re-routes exactly like
/// `on_failure.goto` — the gate fails retryable, control transfers, and
/// once the target's subgraph completes the gate asks again; an
/// unmapped option finishes the gate with that choice as its outcome;
/// `abort` pauses the run. No surface → `Waiting`, with nothing
/// recorded, so a resume re-asks.
#[tracing::instrument(
    name = "resolve_internal_gate",
    skip_all,
    fields(run_id = %ctx.run_id, node_id = %node.id)
)]
pub(super) async fn resolve(ctx: &RunCtx<'_>, node: &Node) -> Result<GateStep, RunError> {
    let broken = |detail: String| RunError::Broken {
        diagnostic: format!("node `{}`'s gate: {detail}", node.id),
    };
    let gate = InternalGate::of(node)
        .ok_or_else(|| broken("the scheduler chose it as an internal gate".to_string()))?;
    let state = ctx.run_view().await?.state;
    let escalation = gate
        .escalation(&ctx.manifest.workflow, &state)
        .map_err(|source| broken(source.to_string()))?;
    if gate.shows_the_run_findings() {
        super::gate_findings::keep(ctx, &state).await?;
    }
    // A decision `resolve_gate` pre-seeded onto the log while this run
    // was parked is consumed here — never re-asked, and its escalation
    // pair is already recorded. Re-validated against the re-derived
    // escalation: a mismatch means ask normally.
    let pre_seeded = super::escalation::pre_seeded_resolution(&state, &node.id, &escalation);
    let already_recorded = pre_seeded.is_some();
    let choice = match pre_seeded {
        Some(choice) => choice,
        None => match ctx.ask_human(&escalation).await? {
            Some(choice) => choice,
            None => {
                return Ok(GateStep::Waiting(PauseReason::Escalation(Box::new(
                    escalation,
                ))))
            }
        },
    };
    let aborted = aborts(&choice.option);
    if !aborted {
        emit_started(ctx, node).await?;
    }
    if !already_recorded {
        record(ctx, node, escalation, &choice).await?;
    }
    if aborted {
        // The usual escalation convention exactly: the interaction is
        // recorded, the run pauses, and the node stays stateless so a
        // resume re-asks if the person changes their mind.
        return Ok(GateStep::Waiting(PauseReason::GateAborted {
            node: node.id.clone(),
            free_text: choice.free_text,
        }));
    }
    land(ctx, &gate, choice).await?;
    Ok(GateStep::Resolved)
}

/// The question and the answer, together, on the log.
async fn record(
    ctx: &RunCtx<'_>,
    node: &Node,
    escalation: Escalation,
    choice: &HumanChoice,
) -> Result<(), RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Waiting(escalation.into_payload())),
    )
    .await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(
            choice.clone(),
        ))),
    )
    .await?;
    Ok(())
}

/// Where `choice` sends the run: back to the node its option names, with
/// the person's words as the cause, or on past a gate it closes.
async fn land(
    ctx: &RunCtx<'_>,
    gate: &InternalGate<'_>,
    choice: HumanChoice,
) -> Result<(), RunError> {
    let node = gate.node;
    let chosen = choice.option;
    let Some(target) = gate.on.get(&chosen) else {
        finish_node(ctx, node, chosen.to_string(), TokenUsage::default()).await?;
        return Ok(());
    };
    // Same shape as any other reroute: the gate fails (retryable — a
    // person chose a correction lap, not a dead end) and control
    // re-routes; the scheduler brings it back to ask again once
    // `target`'s subgraph completes.
    fail(
        ctx,
        node,
        format!("gate chose `{chosen}` — re-routing to `{target}`"),
        true,
    )
    .await?;
    let cause = yunta_core::text::detailed(
        format!("gate `{}` chose `{chosen}`", node.id),
        choice.free_text.as_deref().unwrap_or_default(),
    );
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::Rerouted(
            yunta_core::events::NodeReroutedPayload::new(
                target.clone(),
                yunta_core::events::RerouteCause(yunta_core::events::Failure::message(cause)),
                yunta_core::events::RerouteOrigin::GateChoice,
                None,
                None,
            ),
        )),
    )
    .await?;
    Ok(())
}
