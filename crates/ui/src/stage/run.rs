//! See [`super`]. A run, followed: its chronicle in render's words, the rows a person
//! watches it by under it, its ribbon across the screen and its page beside it, or the
//! page of the node picked in its place.
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

mod acts;

use chrono::{DateTime, Utc};
use surco::components::{Note, Region, Screen, Side};
use surco::doc::{Block, Doc};
use surco::look::Look;
use yunta_api::{FollowedRun, RunNews};
use yunta_core::{NodeId, RunId};
use yunta_render::page::{Node, Page, Run};
use yunta_render::watched::{self, Heard};
use yunta_replay::frame::{chronicle, Moment, RunFrame};

use super::Want;
use crate::prompt::Stroke;
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

    /// The run as a screen at `now`: its ribbon across it, its chronicle and the rows a
    /// person watches it by, its page or the picked node's beside them, and the keys.
    pub(crate) fn screen(&self, look: &Look, now: DateTime<Utc>) -> Screen {
        let frame = self.frame(now);
        let mut screen = Screen::new(Region {
            title: None,
            doc: self.watched(frame.as_ref(), look),
        });
        screen.trail = vec![
            yunta_render::screen::YUNTA.to_string(),
            self.run.handle().to_string(),
        ];
        screen
            .trail
            .extend(self.picked.as_ref().map(|(node, _)| node.to_string()));
        screen.about = frame
            .as_ref()
            .map(|frame| Doc::new().with(watched::ribbon(frame, look.motion)));
        let beside = match &self.picked {
            Some((_, page)) => page.as_ref().map(|page| page.document(look)),
            None => self.page.as_ref().map(|page| page.document(look)),
        };
        screen.side = beside.map(|doc| Side {
            region: Region { title: None, doc },
            focused: false,
        });
        let offered = self.offered(frame.as_ref());
        let acts: Vec<(char, &str)> = offered.iter().map(|act| (act.key, act.does)).collect();
        screen.legend = yunta_render::screen::run_legend(
            self.pointed.as_ref(),
            &acts,
            self.follows,
            self.picked.is_some(),
        );
        screen
    }

    /// The run as a person watches it, framed as `frame`: its chronicle, the rows under
    /// it once the run is read, and what the last piece of work said where it did not
    /// do what was asked.
    fn watched(&self, frame: Option<&RunFrame>, look: &Look) -> Doc {
        let mut doc = yunta_render::chronicle::told(&self.moments, &self.run, look);
        if let (Some(followed), Some(frame)) = (&self.followed, frame) {
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
            doc = doc.with(Block::Lines(lines.collect()));
        }
        match &self.said {
            Some(said) => doc.with(Note(said.clone())),
            None => doc,
        }
    }

    /// What `stroke` asks for here at `now`: moving along the nodes and picking one, an
    /// act the run offers, following it or not, going back, or leaving.
    pub(crate) fn pressed(&mut self, stroke: Stroke, now: DateTime<Utc>) -> Vec<Want> {
        self.said = None;
        match stroke {
            Stroke::Down | Stroke::Insert('j') => self.moved(1, now),
            Stroke::Up | Stroke::Insert('k') => self.moved(-1, now),
            Stroke::Enter => self.picking(),
            Stroke::Insert('f') => self.following_or_not(),
            Stroke::Decline => match self.picked.take() {
                Some(_) => Vec::new(),
                None => vec![Want::Back],
            },
            Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
            Stroke::Insert(key) => {
                let frame = self.frame(now);
                let act = self
                    .offered(frame.as_ref())
                    .into_iter()
                    .find(|act| act.key == key);
                act.map_or_else(Vec::new, |act| acts::taken(act.step))
            }
            _ => Vec::new(),
        }
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
            RunNews::Successor(_) => vec![Want::Work(Operation::ReadPage {
                run: followed.run.clone(),
            })],
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
                if let Some((node, shown)) = &mut self.picked {
                    if page.node.id == *node {
                        *shown = Some(page);
                    }
                }
                Vec::new()
            }
            Outcome::Followed => {
                self.follows = false;
                vec![Want::Work(Operation::ReadPage { run })]
            }
            Outcome::Resumed { .. } | Outcome::Decided { .. } | Outcome::Answered { .. } => {
                self.follows = true;
                vec![
                    Want::Work(Operation::Follow { run: run.clone() }),
                    Want::Work(Operation::ReadPage { run }),
                ]
            }
            Outcome::Cancelled { .. } | Outcome::Closed { .. } => {
                vec![Want::Work(Operation::ReadPage { run })]
            }
            Outcome::Refused(refused) => {
                self.said = Some(refused.to_string());
                Vec::new()
            }
            Outcome::Failed(said) => {
                self.said = Some(said);
                Vec::new()
            }
            Outcome::Repository(_)
            | Outcome::Catalog(_)
            | Outcome::Started { .. }
            | Outcome::Field { .. }
            | Outcome::Questions(_)
            | Outcome::Decision(_)
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// The run's frame at `now`, once its following has read it.
    fn frame(&self, now: DateTime<Utc>) -> Option<RunFrame> {
        self.followed.as_ref().map(|followed| followed.frame(now))
    }

    /// Each act the run framed as `frame` offers that a key here takes: what it offers
    /// by its standing and the decision its page says it stopped on, and answering what
    /// it waits on where it stopped on a menu of its own or on a round of questions.
    fn offered(&self, frame: Option<&RunFrame>) -> Vec<acts::Offered> {
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
                self.picked = None;
                vec![Want::Work(following)]
            }
            false => vec![Want::LetGo(following)],
        }
    }
}
