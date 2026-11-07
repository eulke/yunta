//! A `kind: gate` answered here rather than on a forge: the question it
//! puts, the documents it shows the person deciding, and where their
//! answer sends the run.

use indexmap::IndexMap;
use yunta_core::events::artifacts::ArtifactLedger;
use yunta_core::events::{
    ArtifactId, AuthorGate, Claim, EngineFact, Escalation, EventPayload, Failure, GateEvent,
    GateOption, GateResolvedPayload, HumanChoice, Leads, Menu, NodeEvent, NodeOutcome, PauseReason,
    Shown, TokenUsage, WhyWithheld, Withheld,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::shown::{Flaw, ShownDocument};
use yunta_core::{ArtifactContextRef, Node, NodeId, NodeKind, NonEmpty, OptionId, Seq, Workflow};
use yunta_replay::RunState;

use super::gate_exec::{emit_started, GateStep};
use super::node_close::{fail, finish_node};
use super::{RunCtx, RunError};

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

    /// The escalation a gate puts to a person: `claim` as its question, its
    /// declared options (default: a single `approve`), each saying where it
    /// sends the run — and asking what should change when it sends the run
    /// back to a session — plus `abort`, and the documents `shows` as the
    /// run holds them now.
    ///
    /// `abort` is always the engine's: declaring it only places it on the
    /// menu, and choosing it pauses the run wherever it sits.
    ///
    /// [`InternalGate::asked`] reads the claim and what it shows first, and
    /// is the one way to the escalation a person gets.
    fn escalation(&self, workflow: &Workflow, claim: Claim, shows: Vec<Shown>) -> Escalation {
        let declared: Vec<OptionId> = if self.options.is_empty() {
            vec![ReservedOption::Approve.id()]
        } else {
            self.options.to_vec()
        };
        // Where the run goes once the gate is resolved: the nodes that
        // wait on it.
        let next: Vec<&NodeId> = workflow.dependents(&self.node.id).collect();
        let mut gate_options: Vec<GateOption> = declared
            .iter()
            .map(|id| self.option(workflow, &next, id))
            .collect();
        if !declared.iter().any(aborts) {
            gate_options.push(GateOption::reserved(ReservedOption::Abort));
        }
        let (first, rest) = gate_options
            .split_first()
            .map(|(first, rest)| (first.clone(), rest.to_vec()))
            .unwrap_or_else(|| (GateOption::reserved(ReservedOption::Abort), Vec::new()));
        // The claim is the question; the assignee is the record of who it
        // is addressed to.
        Escalation::new(
            claim,
            Menu::Gate(AuthorGate {
                assignee: self.assignee.to_string(),
                node: Some(self.node.id.clone()),
            }),
            NonEmpty::from((first, rest)),
        )
        .showing(shows)
    }

    /// One declared option: `abort` when its spelling is the reserved one,
    /// and otherwise where its `on:` mapping sends the run. A target the
    /// workflow holds is a `Back`; without one the run goes on past the gate
    /// to `next`, settling the findings the gate shows when it does.
    fn option(&self, workflow: &Workflow, next: &[&NodeId], id: &OptionId) -> GateOption {
        if aborts(id) {
            return GateOption::reserved(ReservedOption::Abort);
        }
        let target = self.on.get(id).and_then(|target| {
            workflow
                .iter_nodes()
                .find(|candidate| candidate.id == *target)
        });
        let leads = match target {
            Some(target) => Leads::Back {
                node: target.id.clone(),
                resumes: target.kind.opens_resumable_session(),
            },
            None => Leads::On {
                next: next.iter().map(|id| (*id).clone()).collect(),
                settles: self.shows_the_run_findings(),
            },
        };
        GateOption::leading(id.clone(), leads)
    }

    /// What the gate asks, read from the documents it shows as the run
    /// in `run_dir` holds them — the one way both the live prompt and a
    /// later rebuild of its menu come by it. What could not be read is
    /// the outer error; what it shows that the run does not hold, or
    /// cannot write out, the inner one.
    ///
    /// A plan that cannot be proven as it is written is never offered to
    /// go on with: the options that take the run past the gate are
    /// withheld, with why, and what is left sends it back or stops it.
    pub(crate) async fn asked(
        &self,
        workflow: &Workflow,
        run_dir: &std::path::Path,
        state: &RunState,
    ) -> Result<Result<Escalation, GateEscalationError>, RunError> {
        let shows = match self.shown(state) {
            Ok(shows) => shows,
            Err(error) => return Ok(Err(error)),
        };
        // A plan is proven by itself and its spec: the run's findings, a
        // view the gate writes only once it asks, prove nothing about it.
        let read: Vec<Shown> = shows
            .iter()
            .filter(|shown| !super::gate_findings::shows_view(shown))
            .cloned()
            .collect();
        let documents = crate::artifacts::shown::documents(run_dir, &read, state, None).await?;
        let flaws: Vec<Flaw> = super::flawed::unprovable(&documents)
            .into_iter()
            .map(|(_, flaw)| flaw)
            .collect();
        // The question is put together from the documents just read, so it
        // costs no read of its own.
        let claim = claim(self.node, self.message, &documents);
        let escalation = self.escalation(workflow, claim, shows);
        Ok(Ok(self.withholding(escalation, NonEmpty::new(flaws))))
    }

    /// `escalation` without the options that go on past the gate, when
    /// what it shows cannot be proven: `flaws` says why, and none leaves it
    /// whole. What is left is each option `on:` sends back, and `abort`.
    fn withholding(&self, escalation: Escalation, flaws: Option<NonEmpty<Flaw>>) -> Escalation {
        let Some(flaws) = flaws else {
            return escalation;
        };
        let withheld = escalation
            .options()
            .iter()
            .filter(|option| !aborts(&option.id) && !self.on.contains_key(&option.id))
            .map(|option| Withheld {
                option: option.id.clone(),
                because: WhyWithheld::Unprovable {
                    flaws: flaws.clone(),
                },
            })
            .collect();
        escalation.withholding(withheld)
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

/// The question a gate asks: its author's `message:` as written, where a blank
/// one counts as none. With none, whether to approve the change the plan among
/// `shown` names, when one names it; else that the gate needs a decision, by
/// its node. An internal gate's escalation and a forge's pull request both ask
/// it. Not a template on `message:`: a message would read an artifact the gate
/// does not name, and every author would write the same sentence.
pub(super) fn claim(node: &Node, message: Option<&str>, shown: &[ShownDocument]) -> Claim {
    match message.filter(|message| !message.trim().is_empty()) {
        Some(message) => Claim::Written(message.to_string()),
        None => match yunta_core::shown::change_shown(shown) {
            Some(summary) => Claim::ApprovePlan {
                summary: summary.to_string(),
            },
            None => Claim::NeedsDecision {
                node: node.id.clone(),
            },
        },
    }
}

/// Where on the log the gate's last decision sits.
async fn decided_at(ctx: &RunCtx<'_>, node: &Node) -> Result<Option<Seq>, RunError> {
    let state = ctx.run_view().await?.state;
    Ok(state
        .gates
        .get(&node.id)
        .and_then(|gate| gate.resolved.last())
        .map(|(_, at)| *at))
}

/// Whether choosing `option` pauses the run.
fn aborts(option: &OptionId) -> bool {
    ReservedOption::of(option) == Some(ReservedOption::Abort)
}

/// Resolves an internal gate: puts its escalation to the person, or
/// takes the decision `resolve_gate` seeded onto the log while the run
/// was parked. An option mapped in `on` reroutes exactly like
/// `on_failure.goto` — the gate fails, control transfers, and
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
        .asked(&ctx.manifest.workflow, ctx.run_dir, &state)
        .await?
        .map_err(|source| unbuilt(node, source))?;
    if gate.shows_the_run_findings() {
        super::gate_findings::keep(ctx, &state).await?;
    }
    let Some((choice, already_recorded)) = choice_at(ctx, node, &escalation, &state).await? else {
        return Ok(GateStep::Waiting(PauseReason::Escalation(Box::new(
            escalation,
        ))));
    };
    let aborted = aborts(&choice.option);
    if !aborted {
        emit_started(ctx, node).await?;
    }
    let shown = escalation.shows().to_vec();
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
    land(ctx, &gate, choice, &shown).await?;
    Ok(GateStep::Resolved)
}

/// The choice that settles a gate now, and whether its escalation and answer
/// are already on the log. A decision `resolve_gate` seeded onto the log while
/// the run was parked is consumed here and never re-asked, provided the
/// escalation derived now still accepts it; otherwise the person is asked.
/// `None` means no surface can ask yet.
async fn choice_at(
    ctx: &RunCtx<'_>,
    node: &Node,
    escalation: &Escalation,
    state: &RunState,
) -> Result<Option<(HumanChoice, bool)>, RunError> {
    let pre_seeded = super::escalation::pre_seeded_resolution(state, &node.id, escalation);
    if let Some((choice, _)) = pre_seeded {
        return Ok(Some((choice, true)));
    }
    let answer = ctx.ask_human(Some(&node.id), None, escalation).await?;
    Ok(answer.map(|choice| (choice, false)))
}

/// Why a gate's question could not be put together, said in words here.
fn unbuilt(node: &Node, source: GateEscalationError) -> RunError {
    RunError::Broken {
        diagnostic: format!("node `{}`'s gate: {source}", node.id),
    }
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
        EventPayload::Gates(GateEvent::Waiting(Box::new(escalation.into_payload()))),
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
    shown: &[Shown],
) -> Result<(), RunError> {
    let node = gate.node;
    let chosen = choice.option;
    let Some(target) = gate.on.get(&chosen) else {
        // Going on past what it showed settles the run's findings it
        // showed: a person read each, with its answers, and went on.
        if let Some(decided) = decided_at(ctx, node).await? {
            super::gate_findings::settle(ctx, node, shown, decided).await?;
        }
        finish_node(
            ctx,
            node,
            NodeOutcome::Fact(EngineFact::OptionChosen { option: chosen }),
            TokenUsage::default(),
        )
        .await?;
        return Ok(());
    };
    route_back_to(ctx, node, &chosen, target, choice.free_text.as_deref()).await
}

/// A correction lap: the gate fails, and control reroutes to
/// `target`. Same shape as any other reroute; the scheduler brings the gate
/// back to ask again once `target`'s subgraph completes.
async fn route_back_to(
    ctx: &RunCtx<'_>,
    node: &Node,
    chosen: &OptionId,
    target: &NodeId,
    free_text: Option<&str>,
) -> Result<(), RunError> {
    // The gate's failure and the reroute's cause are the same fact: what the
    // person chose, where it sends the run, and the words they gave with it.
    let cause = Failure::GateChose {
        node: node.id.clone(),
        option: chosen.clone(),
        target: target.clone(),
        said: free_text.map(str::to_string),
    };
    fail(ctx, node, cause.clone()).await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::Rerouted(
            yunta_core::events::NodeReroutedPayload::new(
                target.clone(),
                cause,
                yunta_core::events::RerouteOrigin::GateChoice,
                None,
                None,
            ),
        )),
    )
    .await?;
    Ok(())
}
