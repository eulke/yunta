//! See [`super`]. A pull request's pane: under the title of the gate that waits on it, the
//! pull request by its number and its link, and how it stands, once its forge answered the
//! poll the scene asked as it opened, as render says it. Before that answer, the title
//! stands alone: nothing is said of the pull request its forge did not answer. No feed
//! closes it: the gate's feed is on its own scene, one `j` or `k` away.

use surco::components::{Note, Region};
use surco::doc::{Block, Doc};
use yunta_core::NodeId;
use yunta_render::blocks::PullRequestReview;

use super::super::{Framed, RunStage};
use super::titled;

impl RunStage {
    /// The pane of the run `framed`, waiting on its pull request, for the node `pointed`: its
    /// title, the pull request and how it stands where its forge answered, `look_failed` said
    /// under it where a look failed.
    pub(super) fn pull_request_pane(
        &self,
        framed: &Framed<'_>,
        pointed: &NodeId,
        look_failed: Option<&String>,
    ) -> Region {
        let review = self.pull_request_polled(framed).map(|polled| {
            PullRequestReview {
                pull_request: &polled.pull_request,
                answered: &polled.answered,
            }
            .blocks()
        });
        let failed = look_failed.map(|failed| Block::from(Note(failed.clone())));
        let doc = Doc::new()
            .with_all(review.into_iter().flatten())
            .with_all(failed);
        titled(Region::new(doc), framed, pointed)
    }
}
