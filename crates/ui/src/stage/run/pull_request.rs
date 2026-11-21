//! See [`super`]. The pull request a run waits on at its gate, as its stage reads it: polled
//! on the run's forge once its scene stands for the pause the run waits at, and kept beside
//! that pause, read once as every reading beyond the log is (`read_once`): never on a draw,
//! on news of the same pause, or as the person moves away and back, and polled again only at
//! another pause, or where a stage opened over the run let the poll go before the forge
//! answered. Its link is the log's, which `o` hands the surface to show whether the forge
//! answered or not (`acts`); a link the surface could not show is noted as a failed act,
//! with the link, until the next key.
//!
//! What the forge answered is drawn in the scene's pane once it came, for the pause it was
//! asked for (`pane`).

use yunta_api::PullRequestPolled;
use yunta_core::events::{Menu, PauseReason};
use yunta_core::Seq;
use yunta_replay::frame::{RunFrame, WaitingOn};

use super::frame::Notice;
use super::scene::ComposedPane;
use super::{Framed, RunStage};
use crate::stage::Want;
use crate::{Address, Operation};

impl RunStage {
    /// The reading of the pull request the run `framed` waits on, asked where its scene
    /// stands as its pull request at a pause it was not asked for: once a pause, and nothing
    /// anywhere else.
    pub(super) fn pull_request_asked(&mut self, framed: Option<&Framed<'_>>) -> Option<Want> {
        let framed = framed?;
        let pane = self.scene_of(framed)?.composes();
        if pane != ComposedPane::PullRequest {
            return None;
        }
        let pause = paused_at(framed)?;
        self.pull_request.asked(pause).then(|| {
            Want::Work(Operation::ReadPullRequest {
                run: self.run.clone(),
            })
        })
    }

    /// What the forge's answer to the pull request the stage asked for asks: nothing, the
    /// answer kept beside the pause it was asked for. None where the run waited at no gate a
    /// forge holds by the time it was read.
    pub(super) fn pull_request_read(
        &mut self,
        polled: Option<Box<PullRequestPolled>>,
    ) -> Vec<Want> {
        self.pull_request.answered(polled);
        Vec::new()
    }

    /// What the surface could not show of what this stage asked it to, `address`: the pull
    /// request's link, noted as a failed act with the link in it, for the person to take
    /// from the screen.
    pub(crate) fn unshown(&mut self, address: &Address) {
        if let Address::Link(link) = address {
            self.notice = Some(Notice::Failed(yunta_render::page::unopened(link)));
        }
    }

    /// What the forge answered of the pull request the run `framed` waits on, where it
    /// answered for the pause the run stands at.
    pub(super) fn pull_request_polled(&self, framed: &Framed<'_>) -> Option<&PullRequestPolled> {
        let pause = paused_at(framed)?;
        self.pull_request.answer(&pause)?.as_deref()
    }
}

/// The pause the run `framed` stands at, by where its log holds it.
fn paused_at(framed: &Framed<'_>) -> Option<Seq> {
    framed.in_view.run.state.run.paused().map(|(_, at)| at)
}

/// The link of the pull request the run framed as `frame` waits on, as its log holds it: of
/// the pull request a forge reviews or a person closed, or the one a gate's escalation asks
/// on. None where it waits on no pull request.
pub(super) fn link(frame: &RunFrame) -> Option<String> {
    let pause = frame.phase.waiting_on().and_then(WaitingOn::reason)?;
    match pause {
        PauseReason::ExternalGate { url } | PauseReason::PullRequestClosed { url, .. } => {
            Some(url.clone())
        }
        PauseReason::Escalation(escalation) => match escalation.menu() {
            Some(Menu::Published(published)) => Some(published.url.clone()),
            _ => None,
        },
        _ => None,
    }
}
