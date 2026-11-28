//! See [`super`]. A run's whole chronicle, as `yunta status <run> --follow` prints it, opened
//! from the run's palette over its screen: every moment of every node and of the run, a feed
//! of its own under a heading that counts them, each row about its node or the run; what
//! stands across the run's screen above it and what the run spent under it; in the run's
//! frame, where the run's own decision stands out of sight.
//!
//! The stage keeps none of the run: the moments are the run in view's, which the state keeps
//! of what it follows under every stage over the run's, so the chronicle grows as the run's
//! news come and nothing is read for it. Until the run in view is the stage's run, it is
//! being read. Render is handed only the moments around where the feed stands
//! (`Scroll::window`): it opens at its end and stays there as it grows; scrolled back it
//! stays on the rows in sight, counting those that come under them, until `End` or scrolling
//! to its last row takes it back to its end. A promotion moves the stage onto the successor
//! (`succeeded`), whose chronicle opens at its end.
//!
//! The arrows, the page keys and the space bar scroll it; `esc` goes back to the run; `a` goes
//! where its attention row names (`over_the_run`); `n` opens a new run over it; `q` goes down
//! to the field, as from every page; Ctrl-C leaves.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use surco::components::{Laid, Legend, Note, Region, Screen};
use surco::doc::{Block, Doc};
use surco::look::Look;
use yunta_core::step::{Arg, NextStep};
use yunta_core::{RunId, SessionId, TaskId};
use yunta_render::{chronicle, screen, watched};
use yunta_replay::frame::Moment;

use super::over_the_run::{attending, in_the_runs_frame};
use super::{ItsRun, Want};
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{FollowedRuns, Outcome, RunInView};

/// A run's whole chronicle, a feed of its own over the run: where the person stands in it.
/// Not the feed a run's pane ends with, one node's.
#[derive(Debug, Clone)]
pub struct ChronicleStage {
    /// The run whose chronicle it is.
    run: RunId,
    /// Where the feed is scrolled to: its end, until a person scrolls it back.
    scroll: Scroll,
    /// What refused or failed, where something did.
    said: Option<String>,
}

impl ChronicleStage {
    /// What it is to its run: a view of it as it moves.
    pub(super) fn its_run(&self) -> ItsRun {
        ItsRun::Shows(self.run.clone())
    }

    /// The whole chronicle of `run`, at its end, made from the run in view once it is `run`.
    pub(crate) fn of(run: RunId) -> Self {
        ChronicleStage {
            run,
            scroll: Scroll::at_the_end(),
            said: None,
        }
    }

    /// The run's whole chronicle of the run in view `followed` keeps at `now`, under its
    /// heading, with what stands across the run's screen and what the run spent; or that it
    /// is being read; what refused or failed; in the run's frame, a line typed in what stands
    /// over it where `typed`.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        let in_view = followed.in_view.as_deref();
        let in_view = in_view.filter(|in_view| *in_view.run.run() == self.run);
        let mut doc = match in_view {
            Some(in_view) => self.told(in_view, look),
            None => Doc::new().with(Note(screen::READING_THE_CHRONICLE.to_string())),
        };
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut shown = Screen::new(Region::new(doc).in_sight(self.scroll.sight()));
        if let Some(in_view) = in_view {
            let (frame, times) = (in_view.run.frame(), in_view.run.times(now));
            shown.about = Some(super::run::screen::across(frame, &times, None, look));
            shown.meter = Some(watched::spent(frame, &times));
        }
        let place = screen::place_of(self.run.handle(), screen::CHRONICLE);
        shown.trail = vec![screen::YUNTA.to_string(), place];
        shown.legend = screen::page_legend(
            screen::PageRead::Scrolled,
            Legend::new(),
            followed.can_ask_for_a_run(),
        );
        in_the_runs_frame(&mut shown, &self.run, followed, now, typed);
        shown
    }

    /// The whole chronicle of the run `in_view`, under its heading, which counts every moment
    /// that says anything: its window where the feed stands, its calls naming the task their
    /// session worked where calls of more than one task stand in it.
    fn told(&self, in_view: &RunInView, look: &Look) -> Doc {
        let told: Vec<&Moment> = in_view
            .moments
            .iter()
            .filter(|moment| chronicle::says_anything(&moment.happening))
            .collect();
        let tasks: BTreeMap<SessionId, TaskId> = in_view
            .run
            .state()
            .nodes
            .values()
            .flat_map(|record| record.session_tasks.clone())
            .collect();
        let held = self.scroll.window(told.len());
        let (before, after) = (held.start, told.len() - held.end);
        let moments: Vec<Moment> = told[held].iter().map(|moment| (*moment).clone()).collect();
        let heading = chronicle::whole_heading(told.len(), look.glyphs);
        Doc::new()
            .with(Block::Heading(heading))
            .with(chronicle::whole(&moments, &tasks, before, after, look))
    }

    /// The stage, where it shows `run`'s chronicle, moved onto `by`, the successor a promotion
    /// handed the work to: its chronicle, at its end, once the run in view is `by`.
    pub(crate) fn succeeded(&mut self, run: &RunId, by: &RunId) {
        if self.run == *run {
            self.run = by.clone();
            self.scroll.back_at_its_end();
        }
    }

    /// What `stroke` asks for here, over what `followed` keeps at `now`: Escape goes back to
    /// the run, `a` where the attention row names, `n` opens a new run over it, `q` goes down
    /// to the field, Ctrl-C leaves, and the keys that scroll scroll the feed.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        match stroke {
            Stroke::Decline => vec![Want::Back],
            Stroke::Insert('a') => attending(&self.run, followed, now),
            Stroke::Insert('n') if followed.can_ask_for_a_run() => Want::taking(&NextStep::Run {
                workflow: Arg::Blank,
            }),
            Stroke::Insert('q') => vec![Want::ToTheField(None)],
            Stroke::Interrupt => vec![Want::Leave],
            other => {
                self.scroll.pressed(other);
                Vec::new()
            }
        }
    }

    /// What a surface laid out of the feed, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }

    /// What an answer to the work the stage asked for does: what refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Refused(refused) => self.said = Some(refused.to_string()),
            Outcome::Failed(said) => self.said = Some(said),
            Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Doctor(_)
            | Outcome::Collected(_)
            | Outcome::Repository(_)
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::PullRequest(_)
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Started { .. }
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Detected { .. }
            | Outcome::SetupPack { .. }
            | Outcome::Initialized { .. }
            | Outcome::Stopped => {}
        }
        Vec::new()
    }
}
