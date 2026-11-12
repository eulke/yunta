//! See [`super`]. A run, followed: its ribbon and the rows a person watches it by across
//! the screen, its nodes beside its page as a list the pointer moves along, and its page
//! with its chronicle under it, or the page of the node picked in its place.
//!
//! The stage asks for the run's following and its page as it opens. The following
//! hands back the run as it reads first, then what each look at it finds, which the
//! stage folds into its copy of the run by the API's own fold, so the chronicle, the
//! rows and the ribbon are the ones `status --follow` draws from the same log. A
//! promotion's successor is followed on and its page read again; once the run stops or
//! stalls, its page is read once more and the stage stops following.
//!
//! A pointer moves along the run's nodes, and picking one stops following and shows its
//! page. The acts the run offers that a key here takes are `acts`': each is bound to its
//! key, and the legend says each in render's words.
//!
//! The page scrolls by the page, the arrows moving along the nodes; it keeps the line it
//! was scrolled to when it is read again, and starts at its first line when it is
//! another page: a node's, the run's again, a successor's.

mod acts;

use chrono::{DateTime, Utc};
use surco::components::{Laid, Note, Region, Screen, Side, Sight};
use surco::doc::{Block, Depth, Doc};
use surco::look::Look;
use yunta_api::{FollowedRun, RunNews};
use yunta_core::step::NextStep;
use yunta_core::{NodeId, RunId};
use yunta_render::page::{Node, Page, Run};
use yunta_render::watched::{self, Heard};
use yunta_replay::frame::{chronicle, Moment, RunFrame};

use super::{Act, Want};
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{News, Operation, Outcome};

/// A run a person follows.
#[derive(Debug, Clone)]
pub struct RunStage {
    /// The run followed: the one opened, or the last a promotion handed the work to.
    run: RunId,
    /// The run as its following read it, once it has.
    followed: Option<Box<FollowedRun>>,
    /// What happened to it, in order, as replay's chronicle folds the events read.
    moments: Vec<Moment>,
    /// What its sessions said last, a frame to each node.
    heard: Heard,
    /// Its page, once read.
    page: Option<Box<Run>>,
    /// Whether news of the run comes while it moves.
    follows: bool,
    /// The node a key acts on, once the run's nodes are read.
    pointed: Option<NodeId>,
    /// The node whose page stands in place of the run's, and that page once read.
    picked: Option<(NodeId, Option<Box<Node>>)>,
    /// Where the page is scrolled to.
    scroll: Scroll,
    /// What the last piece of work came to where it did not do what was asked.
    said: Option<String>,
}

impl RunStage {
    /// The stage of `run`, before anything of it is read.
    pub(crate) fn of(run: RunId) -> Self {
        RunStage {
            run,
            followed: None,
            moments: Vec::new(),
            heard: Heard::default(),
            page: None,
            follows: true,
            pointed: None,
            picked: None,
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What the stage asks for as it opens: the run's following, and its page.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![
            Want::Work(Operation::Follow {
                run: self.run.clone(),
            }),
            Want::Work(Operation::ReadPage {
                run: self.run.clone(),
            }),
        ]
    }

    /// What the stage asks for when a person comes back to it: the following it had,
    /// and the page it showed, read again.
    pub(crate) fn returned(&self) -> Vec<Want> {
        let run = self.run.clone();
        let following = self.follows.then(|| Operation::Follow { run: run.clone() });
        let page = match &self.picked {
            Some((node, _)) => Operation::ReadNodePage {
                run,
                node: node.clone(),
            },
            None => Operation::ReadPage { run },
        };
        following
            .into_iter()
            .chain([page])
            .map(Want::Work)
            .collect()
    }

    /// The run as a screen at `now`: its ribbon and the rows a person watches it by across
    /// it, its nodes beside its page, its page or the picked node's, and the keys.
    pub(crate) fn screen(&self, look: &Look, now: DateTime<Utc>) -> Screen {
        let frame = self.frame(now);
        let page = Region::new(self.read(frame.as_ref(), look)).in_sight(self.scroll.sight());
        let mut screen = Screen::new(page);
        screen.trail = vec![
            yunta_render::screen::YUNTA.to_string(),
            self.run.handle().to_string(),
        ];
        screen
            .trail
            .extend(self.picked.as_ref().map(|(node, _)| node.to_string()));
        screen.about = frame.as_ref().map(|frame| self.across(frame, look));
        screen.side = frame.as_ref().map(|frame| {
            let nodes = watched::nodes(frame, self.pointed.as_ref(), look);
            Side {
                region: Region::new(Doc::new().with(Block::Lines(nodes)))
                    .in_sight(Sight::following_pointer()),
                focused: true,
            }
        });
        let offered = self.offered_in(frame.as_ref());
        let acts: Vec<(char, &str)> = offered.iter().map(|act| (act.key, act.does)).collect();
        let legend = yunta_render::screen::run_legend(
            self.pointed.as_ref(),
            &acts,
            self.follows,
            self.picked.is_some(),
        );
        screen.legend = match self.scroll.scrolls() {
            true => yunta_render::screen::scrolling_by_the_page(legend),
            false => legend,
        };
        screen
    }

    /// What stands across the screen, framed as `frame`: the run's ribbon, then the rows a
    /// person watches it by: what needs them, what each working node is doing, and how far
    /// the run has come.
    fn across(&self, frame: &RunFrame, look: &Look) -> Doc {
        let doc = Doc::new().with(watched::ribbon(frame, look.motion));
        let Some(followed) = &self.followed else {
            return doc;
        };
        let rows = watched::rows(
            frame,
            &self.run,
            followed.offers_a_menu(frame),
            &self.heard,
            look,
        );
        let lines = rows
            .attention
            .into_iter()
            .chain(rows.nodes)
            .chain([rows.footer]);
        doc.with(Block::Lines(lines.collect()))
    }

    /// The page a person reads, framed as `frame`: the picked node's, or the run's with
    /// its chronicle under it, and what the last piece of work said where it did not do
    /// what was asked.
    fn read(&self, frame: Option<&RunFrame>, look: &Look) -> Doc {
        let page = match &self.picked {
            Some((_, page)) => page.as_ref().map(|page| page.document(look)),
            None => self.page.as_ref().map(|page| page.document(look)),
        };
        let mut doc = Doc::new().with_all(page.map(Doc::into_blocks).unwrap_or_default());
        if self.picked.is_none() && frame.is_some() {
            let told = yunta_render::chronicle::told(&self.moments, &self.run, look);
            doc = doc.with_all(told.into_blocks());
        }
        match &self.said {
            Some(said) => doc.with(Note(said.clone())),
            None => doc,
        }
    }

    /// What `stroke` asks for here at `now`: moving along the nodes and picking one, an
    /// act the run offers, following it or not, going back, leaving, or scrolling the
    /// page beside.
    pub(crate) fn pressed(&mut self, stroke: Stroke, now: DateTime<Utc>) -> Vec<Want> {
        self.said = None;
        match stroke {
            Stroke::Down | Stroke::Insert('j') => self.moved(1, now),
            Stroke::Up | Stroke::Insert('k') => self.moved(-1, now),
            Stroke::Enter => self.picking(),
            Stroke::Insert('f') => self.following_or_not(),
            Stroke::Decline => match self.picked.take() {
                Some(_) => {
                    self.scroll.rewound();
                    Vec::new()
                }
                None => vec![Want::Back],
            },
            Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
            Stroke::Insert(key) => {
                let frame = self.frame(now);
                let act = self
                    .offered_in(frame.as_ref())
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

    /// What news of the run's following does at `now`: the run as it reads first, then
    /// each look folded in, by the fold the following itself makes.
    pub(crate) fn heard(&mut self, news: News, now: DateTime<Utc>) -> Vec<Want> {
        let news = match news {
            News::Following(followed) => {
                self.followed = Some(followed);
                self.follows = true;
                self.read_on(now);
                return Vec::new();
            }
            News::Run(news) => news,
            News::CasesNamed(_) | News::Case(_) => return Vec::new(),
        };
        let Some(followed) = self.followed.as_mut() else {
            return Vec::new();
        };
        followed.took(&news);
        let wants = match news {
            RunNews::Said(said) => {
                self.heard.hear(said);
                Vec::new()
            }
            RunNews::Successor(_) => {
                self.scroll.rewound();
                vec![Want::Work(Operation::ReadPage {
                    run: followed.run.clone(),
                })]
            }
            RunNews::Events(_) | RunNews::Engine(_) | RunNews::Stopped | RunNews::Stalled => {
                Vec::new()
            }
        };
        self.read_on(now);
        wants
    }

    /// The run, its moments, what its sessions said and the node pointed at kept to
    /// what was read at `now`.
    fn read_on(&mut self, now: DateTime<Utc>) {
        let Some(followed) = &self.followed else {
            return;
        };
        self.run = followed.run.clone();
        self.moments = chronicle(&followed.events);
        let frame = followed.frame(now);
        self.heard.kept_to(&frame, &self.run);
        let held = self
            .pointed
            .as_ref()
            .is_some_and(|node| frame.nodes.iter().any(|framed| framed.id == *node));
        if !held {
            self.pointed = frame.nodes.first().map(|node| node.id.clone());
        }
    }

    /// What an answer to work the stage asked for does: a page read, or, once the run
    /// no longer moves, its page read once more; after an act, the run read again;
    /// what refused or failed is said.
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
            Outcome::Followed => {
                self.follows = false;
                vec![Want::Work(Operation::ReadPage { run })]
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
            | Outcome::Field { .. }
            | Outcome::Questions(_)
            | Outcome::Decision(_)
            | Outcome::Stats(_)
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Doctor(_)
            | Outcome::Collected(_)
            | Outcome::Detected { .. }
            | Outcome::Initialized(_)
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// The run went on, here: it is followed again, and its own page stands in place of
    /// a node's, read again.
    fn went_on(&mut self) -> Vec<Want> {
        self.follows = true;
        if self.picked.take().is_some() {
            self.scroll.rewound();
        }
        let run = self.run.clone();
        vec![
            Want::Work(Operation::Follow { run: run.clone() }),
            Want::Work(Operation::ReadPage { run }),
        ]
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

    /// What the run offers at `now`, each with the key that takes it here: every act its
    /// frame offers that a key here takes, answering what it waits on first, then the
    /// page of the node pointed at.
    pub(crate) fn offered(&self, now: DateTime<Utc>) -> Vec<Act> {
        let frame = self.frame(now);
        let node_page = self
            .pointed
            .clone()
            .filter(|_| self.picked.is_none())
            .map(|node| NextStep::Status {
                run: self.run.clone(),
                node: Some(node),
                depth: Depth::Glance,
            });
        let node_page = node_page.map(|step| Act::by(Stroke::Enter, step));
        let acts = self
            .offered_in(frame.as_ref())
            .into_iter()
            .map(|act| Act::by(Stroke::Insert(act.key), act.step));
        acts.chain(node_page).collect()
    }

    /// The run's frame at `now`, once its following has read it.
    fn frame(&self, now: DateTime<Utc>) -> Option<RunFrame> {
        self.followed.as_ref().map(|followed| followed.frame(now))
    }

    /// Each act the run framed as `frame` offers that a key here takes: what it offers
    /// by its standing and the decision its page says it stopped on, and answering what
    /// it waits on where it stopped on a menu of its own or on a round of questions.
    fn offered_in(&self, frame: Option<&RunFrame>) -> Vec<acts::Offered> {
        let (Some(frame), Some(followed)) = (frame, &self.followed) else {
            return Vec::new();
        };
        let decision = self
            .page
            .as_ref()
            .and_then(|page| page.decision.as_ref())
            .map(|decision| &decision.escalation);
        let answerable = followed.offers_a_menu(frame) || yunta_api::waiting_round(frame).is_some();
        acts::offered(frame, decision, answerable)
    }

    /// The pointer moved `by` nodes along the run's nodes at `now`, around at either
    /// end.
    fn moved(&mut self, by: isize, now: DateTime<Utc>) -> Vec<Want> {
        let Some(frame) = self.frame(now) else {
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

    /// The node pointed at picked: following stops, and its page is read to stand in
    /// place of the run's.
    fn picking(&mut self) -> Vec<Want> {
        let Some(node) = self.pointed.clone() else {
            return Vec::new();
        };
        self.picked = Some((node.clone(), None));
        self.scroll.rewound();
        let run = self.run.clone();
        let mut wants = vec![Want::Work(Operation::ReadNodePage {
            run: run.clone(),
            node,
        })];
        if std::mem::replace(&mut self.follows, false) {
            wants.push(Want::LetGo(Operation::Follow { run }));
        }
        wants
    }

    /// Following the run, or no longer: following again shows the run's page in place
    /// of a node's.
    fn following_or_not(&mut self) -> Vec<Want> {
        self.follows = !self.follows;
        let following = Operation::Follow {
            run: self.run.clone(),
        };
        match self.follows {
            true => {
                if self.picked.take().is_some() {
                    self.scroll.rewound();
                }
                vec![Want::Work(following)]
            }
            false => vec![Want::LetGo(following)],
        }
    }
}
