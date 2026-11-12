//! See [`super`]. A decision put to a person, owned: what its card and the plan behind
//! it are built from, alike at a terminal and in the application, wherever the decision
//! was read.

use std::collections::BTreeMap;

use yunta_api::WaitingDecision;
use yunta_core::events::{ChangesCountedPayload, GateWaitingPayload};
use yunta_core::port::DecisionAsked;
use yunta_core::shown::ShownDocument;
use yunta_core::{NodeId, OptionId, RunId};
use yunta_render::escalation::Route;

/// A decision put to a person: the escalation, the documents it shows in the run they
/// belong to, what the run changed, what follows each answer, the node it is on, and
/// the workflow and mode the run follows. Not core's [`DecisionAsked`], what the engine
/// driving a run hands a person beside the escalation, which this is put from.
#[derive(Debug, Clone)]
pub struct DecisionPut {
    pub escalation: GateWaitingPayload,
    /// The documents it shows, in the run they belong to.
    pub shown: Option<(RunId, Vec<ShownDocument>)>,
    /// What the run changed, when the change was counted.
    pub changed: Option<ChangesCountedPayload>,
    /// What follows each option that leads somewhere.
    pub routes: BTreeMap<OptionId, Route>,
    /// The node it is on.
    pub node: Option<NodeId>,
    /// The workflow and the mode the run follows, as a person reads them.
    pub called: Option<String>,
}

impl DecisionPut {
    /// `escalation` put with nothing beside it: no document, no change, and nothing said
    /// of what follows an answer.
    pub fn alone(escalation: GateWaitingPayload) -> Self {
        DecisionPut {
            escalation,
            shown: None,
            changed: None,
            routes: BTreeMap::new(),
            node: None,
            called: None,
        }
    }

    /// The decision `run` waits on, as its log rebuilds it.
    pub fn waiting(run: &RunId, waiting: &WaitingDecision) -> Self {
        DecisionPut {
            escalation: waiting.escalation.clone(),
            shown: Some((run.clone(), waiting.shown.clone())),
            changed: waiting.changed.clone(),
            routes: waiting.routes.clone(),
            node: waiting.on.node().cloned(),
            called: Some(waiting.called.clone()),
        }
    }

    /// `escalation` as the engine driving its run asks it, with what `asking` says
    /// beside it.
    pub fn asked(escalation: &GateWaitingPayload, asking: &DecisionAsked<'_>) -> Self {
        DecisionPut {
            escalation: escalation.clone(),
            shown: Some((asking.run.clone(), asking.shown.to_vec())),
            changed: asking.changed.cloned(),
            routes: yunta_render::escalation::routes(
                escalation,
                asking.workflow,
                asking.left_out,
                asking.node,
            ),
            node: asking.node.cloned(),
            called: Some(yunta_render::state::called(
                &asking.workflow.name,
                asking.mode,
            )),
        }
    }

    /// The documents it shows; none where it shows none.
    pub fn documents(&self) -> &[ShownDocument] {
        self.shown
            .as_ref()
            .map_or(&[], |(_, documents)| documents.as_slice())
    }

    /// The run its documents belong to, where it shows any.
    pub fn run(&self) -> Option<&RunId> {
        self.shown.as_ref().map(|(run, _)| run)
    }
}
