//! See [`super`]. A run, followed: its ribbon and the rows a person watches it by across
//! the screen, its nodes beside its page as a list the pointer moves along, and its page
//! with its chronicle under it, or the page of the node picked in its place.
//!
//! The stage keeps none of the run: its ribbon, its rows, its nodes and its chronicle are
//! the run in view's, which the state keeps of what it follows, drawn where it is the
//! stage's run and framed at the instant of each draw. They move as the run's news come,
//! a node picked or not, and its ages move with the clock. The moments are the ones the
//! state's fold tells, so the chronicle, the rows and the ribbon are the ones `status
//! --follow` draws from the same log; whether the run offers a menu is the run in view's,
//! worked out once for its head, and so is the decision its acts are chosen by: what the
//! run in view waits on, once read for the pause it stands at, never its page's. A look
//! that failed is said under the page, over the run as it was last read, until the next
//! news.
//!
//! The stage asks for the run's page as it opens, and nothing to follow: the state follows
//! the stage's run under every stage over it, so no key starts or stops it. Where the work
//! goes on in a promotion's successor, the state moves the stage onto it as it folds the
//! successor (`succeeded`): the successor's own page stands in place of a node's, read
//! again as the page of another run.
//!
//! The run's page stands at the run as kept, its headline, furrow and gist the kept
//! frame's at each draw, and the page shown is read again when the run turns (`page`).
//!
//! A pointer moves along the run's nodes and is kept by the node it stands on; where the
//! run no longer holds that node, its first node stands for it. Picking one shows its
//! page, letting nothing go, while the rows go on moving. The acts the run offers that a
//! key here takes are `acts`': each is bound to its key, and the legend says each in
//! render's words.
//!
//! The page scrolls by the page, the arrows moving along the nodes; it keeps the line it
//! was scrolled to when it is read again, and starts at its first line when it is
//! another page: a node's, the run's again, a successor's.

mod acts;
mod page;

use chrono::{DateTime, Utc};
use surco::components::{Laid, Note, Region, Screen, Side, Sight};
use surco::doc::{Block, Depth, Doc};
use surco::look::Look;
use yunta_api::Awaited;
use yunta_core::step::NextStep;
use yunta_core::{NodeId, RunId};
use yunta_render::page::{Node, Page, Run};
use yunta_render::watched;
use yunta_replay::frame::RunFrame;

use super::{Act, Want};
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{AwaitedReading, FollowedRuns, Operation, Outcome, RunInView};

/// A run a person follows: where they stand on it. Not the run, which the state keeps of
/// what it follows.
#[derive(Debug, Clone)]
pub struct RunStage {
    /// The run followed: the one opened, or the last a promotion handed the work to.
    pub(super) run: RunId,
    /// Its page, once read.
    page: Option<Box<Run>>,
    /// Where the run as kept stood turned when the page shown was last asked: none
    /// before the run is kept.
    turned: Option<page::Turn>,
    /// The node a key acts on, where a person moved onto one: where the run does not hold
    /// it, its first node stands for it.
    pointed: Option<NodeId>,
    /// The node whose page stands in place of the run's, and that page once read.
    picked: Option<(NodeId, Option<Box<Node>>)>,
    /// Where the page is scrolled to.
    scroll: Scroll,
    /// What the last piece of work came to where it did not do what was asked.
    said: Option<String>,
}

/// The run in view, framed at an instant: what the stage draws and acts on.
struct Framed<'a> {
    in_view: &'a RunInView,
    frame: RunFrame,
}

impl RunStage {
    /// The stage of `run`, before anything of it is read.
    pub(crate) fn of(run: RunId) -> Self {
        RunStage {
            run,
            page: None,
            turned: None,
            pointed: None,
            picked: None,
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What the stage asks for as it opens: the run's page.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![Want::Work(Operation::ReadPage {
            run: self.run.clone(),
        })]
    }

    /// What the stage asks for when a person comes back to it: the page it showed, read
    /// again at the run as `followed` keeps it.
    pub(crate) fn returned(&mut self, followed: &FollowedRuns) -> Vec<Want> {
        self.turned = self.kept_turn(followed);
        vec![Want::Work(self.page_shown())]
    }

    /// The run in view `followed` keeps, framed at `now`, where it is this stage's run:
    /// none until it is read.
    fn framed<'a>(&self, followed: &'a FollowedRuns, now: DateTime<Utc>) -> Option<Framed<'a>> {
        let in_view = followed.in_view.as_deref()?;
        (in_view.run.run == self.run).then(|| Framed {
            frame: in_view.run.frame(now),
            in_view,
        })
    }

    /// The node a key acts on among the nodes of `frame`: the one pointed at where the run
    /// holds it, its first node otherwise.
    fn pointed_in(&self, frame: &RunFrame) -> Option<NodeId> {
        let held = |node: &&NodeId| frame.nodes.iter().any(|framed| framed.id == **node);
        let pointed = self.pointed.as_ref().filter(held);
        pointed
            .or(frame.nodes.first().map(|node| &node.id))
            .cloned()
    }

    /// The run `followed` keeps as a screen at `now`: its ribbon and the rows a person
    /// watches it by across it, its nodes beside its page, its page or the picked node's,
    /// and the keys.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Screen {
        let framed = self.framed(followed, now);
        let read = self.read(framed.as_ref(), followed.look_failed.as_ref(), look);
        let mut screen = Screen::new(Region::new(read).in_sight(self.scroll.sight()));
        screen.trail = vec![
            yunta_render::screen::YUNTA.to_string(),
            self.run.handle().to_string(),
        ];
        screen
            .trail
            .extend(self.picked.as_ref().map(|(node, _)| node.to_string()));
        let pointed = framed
            .as_ref()
            .and_then(|framed| self.pointed_in(&framed.frame));
        screen.about = framed.as_ref().map(|framed| self.across(framed, look));
        screen.side = framed.as_ref().map(|framed| {
            let nodes = watched::nodes(&framed.frame, pointed.as_ref(), look);
            Side {
                region: Region::new(Doc::new().with(Block::Rows(nodes)))
                    .in_sight(Sight::following_pointer()),
                focused: true,
            }
        });
        let offered = self.offered_in(framed.as_ref());
        let acts: Vec<(char, &str)> = offered.iter().map(|act| (act.key, act.does)).collect();
        let legend =
            yunta_render::screen::run_legend(pointed.as_ref(), &acts, self.picked.is_some());
        screen.legend = match self.scroll.scrolls() {
            true => yunta_render::screen::scrolling_by_the_page(legend),
            false => legend,
        };
        screen
    }

    /// What stands across the screen of the run `framed`: its ribbon, then the rows a
    /// person watches it by: what needs them, what each working node is doing with what
    /// its session said last, and how far the run has come.
    fn across(&self, framed: &Framed<'_>, look: &Look) -> Doc {
        let Framed { in_view, frame } = framed;
        let ribbon = watched::ribbon(frame, look.motion);
        let rows = watched::rows(frame, &self.run, in_view.run.menu, &in_view.heard, look);
        let lines = rows
            .attention
            .into_iter()
            .chain(rows.nodes)
            .chain([watched::footer(frame, look)]);
        Doc::new().with(ribbon).with(Block::Lines(lines.collect()))
    }

    /// The page a person reads of the run `framed`: the picked node's, or the run's,
    /// standing at the run as framed, with its chronicle under it; then the look that
    /// failed, and what the last piece of work said where it did not do what was asked.
    fn read(&self, framed: Option<&Framed<'_>>, look_failed: Option<&String>, look: &Look) -> Doc {
        let page = match &self.picked {
            Some((_, page)) => page.as_ref().map(|page| page.document(look)),
            None => self.page.as_ref().map(|page| {
                let kept = framed.and_then(|framed| framed.page(page));
                kept.as_ref().unwrap_or(page).document(look)
            }),
        };
        let mut doc = Doc::new().with_all(page.map(Doc::into_blocks).unwrap_or_default());
        if let (None, Some(framed)) = (&self.picked, framed) {
            let told = yunta_render::chronicle::told(&framed.in_view.moments, &self.run, look);
            doc = doc.with_all(told.into_blocks());
        }
        for said in [look_failed, self.said.as_ref()].into_iter().flatten() {
            doc = doc.with(Note(said.clone()));
        }
        doc
    }

    /// What `stroke` asks for here, on the run `followed` keeps at `now`, the pointer
    /// first kept to its nodes: moving along the nodes and picking one, an act the run
    /// offers, going back, leaving, or scrolling the page beside.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        self.said = None;
        let framed = self.framed(followed, now);
        if let Some(framed) = &framed {
            self.pointed = self.pointed_in(&framed.frame);
        }
        match stroke {
            Stroke::Down | Stroke::Insert('j') => self.moved(1, framed.as_ref()),
            Stroke::Up | Stroke::Insert('k') => self.moved(-1, framed.as_ref()),
            Stroke::Enter => self.picking(),
            Stroke::Decline => match self.picked.take() {
                Some(_) => {
                    self.scroll.rewound();
                    Vec::new()
                }
                None => vec![Want::Back],
            },
            Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
            Stroke::Insert(key) => {
                let act = self
                    .offered_in(framed.as_ref())
                    .into_iter()
                    .find(|act| act.key == key);
                match act {
                    Some(act) => acts::taken(act.step),
                    None => {
                        self.scroll.pressed(stroke);
                        Vec::new()
                    }
                }
            }
            other => {
                self.scroll.pressed(other);
                Vec::new()
            }
        }
    }

    /// What a surface laid out of the page, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }

    /// The stage, where its run is `run`, moved onto `by`, the successor a promotion
    /// handed the work to: the successor's own page stands in place of a node's, from its
    /// first line, read once the state says what it folded or the person comes back.
    pub(crate) fn succeeded(&mut self, run: &RunId, by: &RunId) {
        if self.run != *run {
            return;
        }
        self.run = by.clone();
        self.picked = None;
        self.scroll.rewound();
    }

    /// What an answer to work the stage asked for does: a page read; after an act, the
    /// run read again; what refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        let run = self.run.clone();
        match outcome {
            Outcome::Page(page) => {
                self.page = Some(page);
                Vec::new()
            }
            Outcome::NodePage(page) => {
                self.node_page_read(page);
                Vec::new()
            }
            // What another run went on to is said where it is followed, not here.
            Outcome::Resumed { run: went_on }
            | Outcome::Decided { run: went_on }
            | Outcome::Answered { run: went_on }
                if went_on == run =>
            {
                self.went_on()
            }
            Outcome::Cancelled { .. } | Outcome::Closed { .. } => {
                vec![Want::Work(Operation::ReadPage { run })]
            }
            Outcome::Refused(refused) => self.said_that(refused.to_string()),
            Outcome::Failed(said) => self.said_that(said),
            Outcome::Repository(_)
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::Started { .. }
            | Outcome::Resumed { .. }
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Questions(_)
            | Outcome::Decision(_)
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Doctor(_)
            | Outcome::Collected(_)
            | Outcome::Detected { .. }
            | Outcome::Initialized(_)
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// The run went on, here: its own page stands in place of a node's, read again.
    fn went_on(&mut self) -> Vec<Want> {
        if self.picked.take().is_some() {
            self.scroll.rewound();
        }
        vec![Want::Work(Operation::ReadPage {
            run: self.run.clone(),
        })]
    }

    /// `said`, what refused or failed, said where the run is.
    fn said_that(&mut self, said: String) -> Vec<Want> {
        self.said = Some(said);
        Vec::new()
    }

    /// The page of a node, read: it stands where its node is still the one picked.
    fn node_page_read(&mut self, page: Box<Node>) {
        if let Some((node, shown)) = &mut self.picked {
            if page.node.id == *node {
                *shown = Some(page);
            }
        }
    }

    /// What the run `followed` keeps offers at `now`, each with the key that takes it
    /// here: every act its frame offers that a key here takes, answering what it waits on
    /// first, then the page of the node pointed at.
    pub(crate) fn offered(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Act> {
        let framed = self.framed(followed, now);
        let node_page = framed
            .as_ref()
            .and_then(|framed| self.pointed_in(&framed.frame))
            .filter(|_| self.picked.is_none())
            .map(|node| NextStep::Status {
                run: self.run.clone(),
                node: Some(node),
                depth: Depth::Glance,
            });
        let node_page = node_page.map(|step| Act::by(Stroke::Enter, step));
        let acts = self
            .offered_in(framed.as_ref())
            .into_iter()
            .map(|act| Act::by(Stroke::Insert(act.key), act.step));
        acts.chain(node_page).collect()
    }

    /// Each act the run `framed` offers that a key here takes: what it offers by its
    /// standing and the decision it waits on, once the state read it for the pause the
    /// run stands at, and answering what it waits on where it stopped on a menu of its
    /// own, as kept for its head, or on a round of questions.
    fn offered_in(&self, framed: Option<&Framed<'_>>) -> Vec<acts::Offered> {
        let Some(Framed { in_view, frame }) = framed else {
            return Vec::new();
        };
        let paused_at = in_view.run.state.run.paused().map(|(_, at)| at);
        let decision = match &in_view.awaited {
            Some(AwaitedReading::Read {
                pause,
                awaited: Awaited::Decision(decision),
            }) if Some(pause.seq()) == paused_at => Some(&decision.escalation),
            _ => None,
        };
        let answerable = in_view.run.menu || yunta_api::waiting_round(frame).is_some();
        acts::offered(frame, decision, answerable)
    }

    /// The pointer moved `by` nodes along the nodes of the run `framed`, around at either
    /// end.
    fn moved(&mut self, by: isize, framed: Option<&Framed<'_>>) -> Vec<Want> {
        let Some(Framed { frame, .. }) = framed else {
            return Vec::new();
        };
        let nodes: Vec<&NodeId> = frame.nodes.iter().map(|node| &node.id).collect();
        if nodes.is_empty() {
            return Vec::new();
        }
        let at = self
            .pointed
            .as_ref()
            .and_then(|pointed| nodes.iter().position(|node| *node == pointed))
            .unwrap_or(0);
        let to = (at as isize + by).rem_euclid(nodes.len() as isize) as usize;
        self.pointed = Some(nodes[to].clone());
        Vec::new()
    }

    /// The node pointed at picked: the node's page is read to stand in place of the run's,
    /// nothing let go, the rows moving on with what the state follows.
    fn picking(&mut self) -> Vec<Want> {
        let Some(node) = self.pointed.clone() else {
            return Vec::new();
        };
        self.picked = Some((node.clone(), None));
        self.scroll.rewound();
        vec![Want::Work(Operation::ReadNodePage {
            run: self.run.clone(),
            node,
        })]
    }
}
