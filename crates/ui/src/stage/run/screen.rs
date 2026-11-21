//! See [`super`]. A run's screen as its stage lays it out: the trail, which names the run and
//! the node pointed at, or the scene where it names itself, after each run whose child it
//! shows and the node that bore the child; what stands across, its headline, its ribbon and
//! its gist, the receipt's where its scene shows a receipt whose work holds; its nodes beside
//! the pane at the left, the pointer kept in sight, where a round or the plan's desk does not
//! take their place too; the pane its scene composes; what it has spent under them; and all
//! of it in its frame (`frame`). The plan's desk heads itself: nothing of the run stands
//! across it, beside it or under it, between the rule and the attention row. What stands
//! across a run's screen stands across its whole chronicle too ([`across`]).

use chrono::{DateTime, Utc};
use surco::components::{Region, Screen, Side, SideAt, Sight};
use surco::doc::{Block, Doc};
use surco::line::{Line, Tone};
use surco::look::Look;
use yunta_core::NodeId;
use yunta_render::watched;
use yunta_replay::frame::RunFrame;

use super::{Framed, RunStage, Scene};
use crate::FollowedRuns;

impl RunStage {
    /// The run `followed` keeps as a screen at `now`: its headline, ribbon and gist across
    /// it, its nodes beside its pane, the pane its scene composes, what it has spent under
    /// them, but where its scene heads itself, in its frame, a line typed in what stands over
    /// it where `typed`.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        let framed = self.framed(followed, now);
        let read = self.read(framed.as_ref(), followed.look_failed.as_ref(), look);
        let sight = self.pane_in(framed.as_ref()).sight();
        let mut screen = Screen::new(read.in_sight(sight));
        let pointed = framed.as_ref().and_then(|framed| self.pointed_in(framed));
        let scene = framed.as_ref().and_then(|framed| self.scene_of(framed));
        let place = match scene.as_ref().and_then(Scene::place) {
            Some(scene) => Some(scene.to_string()),
            None => pointed.as_ref().map(NodeId::to_string),
        };
        screen.trail = [yunta_render::screen::YUNTA.to_string()]
            .into_iter()
            .chain(self.trail_up())
            .chain([self.run.handle().to_string()])
            .chain(place)
            .collect();
        let heads_itself = scene.as_ref().is_some_and(Scene::heads_itself);
        let across_it = framed.as_ref().filter(|_| !heads_itself);
        screen.about = across_it.map(|framed| {
            let gist = self.receipt_gist(framed);
            let gist = gist.map(|gist| Line::new().push(Tone::Muted, gist));
            across(&framed.frame, gist, look)
        });
        let lists_its_nodes = scene.as_ref().is_none_or(Scene::lists_its_nodes);
        screen.side = framed
            .as_ref()
            .filter(|_| lists_its_nodes)
            .map(|framed| self.nodes_beside(framed, pointed.as_ref(), look));
        screen.meter = across_it.map(|framed| watched::spent(&framed.frame));
        self.in_its_frame(&mut screen, framed.as_ref(), followed, typed, look, now);
        screen
    }

    /// The nodes of the run `framed` beside its pane at the left, `pointed` pointed at, the
    /// pointer kept in sight, drawn in `look`.
    fn nodes_beside(&self, framed: &Framed<'_>, pointed: Option<&NodeId>, look: &Look) -> Side {
        let run = &framed.in_view.run;
        let workflow = &run.manifest.workflow;
        let leads = self.leads();
        let nodes = watched::nodes(&framed.frame, workflow, &run.state, pointed, leads, look);
        Side {
            region: Region::new(Doc::new().with(Block::Rows(nodes)))
                .in_sight(Sight::following_pointer()),
            at: SideAt::Left,
            focused: true,
        }
    }
}

/// What stands across the screen of the run `frame`: its headline, its ribbon with each
/// stretch named, and `gist`, the gist its scene says, else the run's, a row of its own
/// however long, blank where the run's word says it all.
pub(in crate::stage) fn across(frame: &RunFrame, gist: Option<Line>, look: &Look) -> Doc {
    let gist = gist.or_else(|| watched::gist(frame)).unwrap_or_default();
    Doc::new()
        .with(watched::headline(frame))
        .with(watched::ribbon(frame, look.motion))
        .with(Block::Rows(vec![gist]))
}
