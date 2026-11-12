//! See [`super`]. A run read at one instant: the state and frame its log
//! derives, the decision it stopped on, and the tree and branch it works in —
//! what every page and document about a run is drawn from, read once.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use yunta_core::events::{DecisionOn, Escalation, NodeState};
use yunta_core::shown::ShownDocument;
use yunta_core::NodeId;
use yunta_replay::frame::{NodeStanding, RunFrame};
use yunta_replay::RunState;

use super::OpenedRun;
use crate::Error;

/// A run read at one instant: the state and frame its log derives, the
/// decision it stopped on, and the tree and branch it works in.
///
/// Not render's run page, which a person reads drawn from it.
pub struct RunReading<'p> {
    pub run: OpenedRun<'p>,
    pub state: RunState,
    pub frame: RunFrame,
    /// The decision the run stopped on, rebuilt from its state and the
    /// documents its gate shows: `None` for a run that stopped on none.
    pub decision: Option<(DecisionOn, Escalation)>,
    /// The tree the run works in.
    pub tree: PathBuf,
    /// The run's own branch, which its tree works on.
    pub branch: String,
    /// The instant it was read at.
    pub now: DateTime<Utc>,
}

impl<'p> OpenedRun<'p> {
    /// Where the run stands at the project's clock's instant, as replay frames
    /// its log beside the engine its lock names: what a drawing of the run's
    /// graph reads, with no decision rebuilt and no estimate read.
    pub fn frame(&self) -> RunFrame {
        let state = RunState::replay(&self.events);
        self.frame_of(&state, self.project.workspace.clock.now())
    }

    /// The run as its own log describes it at the project's clock's instant.
    pub async fn reading(self) -> Result<RunReading<'p>, Error> {
        let project = self.project;
        let manifest = &self.manifest.doc;
        let now = project.workspace.clock.now();
        let state = RunState::replay(&self.events);
        let decision = yunta_engine::current_escalation(manifest, &self.dir, &state).await?;
        let mut frame = self.frame_of(&state, now);
        frame.prior = project.budget_prior(manifest, &frame, &self.events).await;
        let bound = state.run.checkout().map(Path::to_path_buf);
        let tree = project.run_tree(manifest, bound.as_deref(), &self.id, &project.workspace.dir);
        Ok(RunReading {
            branch: self.id.branch(),
            run: self,
            state,
            frame,
            decision,
            tree,
            now,
        })
    }

    /// The frame `state` derives at `now`, beside the engine the run's lock
    /// names.
    pub(crate) fn frame_of(&self, state: &RunState, now: DateTime<Utc>) -> RunFrame {
        let engine = self.registry().engine();
        let workflow = &self.manifest.doc.workflow;
        RunFrame::of(&self.id, workflow, state, None, &engine, now)
    }
}

impl RunReading<'_> {
    /// The decision the run waits on. `None` for a run that is not parked,
    /// and for a pause that rebuilds no menu: what that one waits on is said
    /// by the frame.
    pub fn waits_on(&self) -> Option<(&DecisionOn, &Escalation)> {
        self.frame.phase.waiting_on()?;
        self.decision
            .as_ref()
            .map(|(on, escalation)| (on, escalation))
    }

    /// The documents the decision the run waits on shows, as the run holds
    /// them: none when it waits on none.
    pub async fn shown(&self) -> Result<Vec<ShownDocument>, Error> {
        let Some((_, escalation)) = self.waits_on() else {
            return Ok(Vec::new());
        };
        let manifest = &self.run.manifest.doc;
        let shows = escalation.shows();
        let documents =
            yunta_engine::shown_documents(&self.run.dir, shows, &self.state, manifest.request());
        Ok(documents.await?)
    }

    /// The documents node `node` produced, as the run holds them.
    pub async fn produced(&self, node: &NodeId) -> Result<Vec<ShownDocument>, Error> {
        let manifest = &self.run.manifest.doc;
        let documents =
            yunta_engine::produced_documents(&self.run.dir, node, &self.state, manifest.request());
        Ok(documents.await?)
    }

    /// The change each finished node's accepted plan says it makes, in one
    /// line, node by node. A node still at work keeps what its row says of
    /// that work, and a node whose documents cannot be read keeps what it
    /// said: what is drawn from this draws either way.
    pub async fn plans(&self) -> Vec<(NodeId, String)> {
        let finished = self.frame.nodes.iter().filter(|node| {
            matches!(
                node.state,
                NodeStanding::Reached(NodeState::Finished { .. })
            )
        });
        let mut summaries = Vec::new();
        for node in finished {
            let Ok(documents) = self.produced(&node.id).await else {
                continue;
            };
            if let Some(change) = yunta_core::shown::change_shown(&documents) {
                summaries.push((node.id.clone(), surco::text::one_line(change)));
            }
        }
        summaries
    }
}
