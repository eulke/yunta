//! See [`super`]. A node's pane as its scene composes it from render's parts of the run in
//! view: its title, the node and its kind with who works it at the far end, then what the
//! node does, then its feed. The composer holds no sentence: every word is render's, and it
//! decides only which part stands where, and which give way first where the pane is short
//! of rows. Before the run is read no scene stands, and the pane says only a look that failed.
//!
//! The work scene's pane says what its node does by what the node is: what a command node or
//! a check runs, as render reads it from the node's declaration, the run's frozen
//! configuration and what its lineage measured; what a session's node hands over, each
//! artifact it declares with where it stands on its open attempt, an accepted one that
//! cannot be read here said with why. A look that failed is said under them, and the feed
//! closes the pane, under its heading, which counts the open attempt's calls, once the feed
//! lists anything: a node not started yet shows what runs first, its feed with no row yet.
//! Work standing still says above all of it what holds it and what moves it again, as
//! render says it; where the run leads and what it waits on a person for could not be read
//! for the pause it stands at, a card stands where its own would, saying why, with nothing
//! to choose, in place of the feed.
//! A loop's pane is its board (`board`); the findings' pane lists the run's findings over
//! the node's feed, with no title (`findings`); a node's own pane says how the node the
//! person points at stands, over its feed whole (`node`); a standstill's pane says why the
//! run stands with nothing to answer here and what moves it again, with no feed
//! (`standstill`); a pull request's pane says the pull request its gate waits on and how it
//! stands, once its forge answered, with no feed (`pull_request`); a run asked elsewhere's
//! pane says the question its process asks at its own terminal, who asks and since when,
//! and that nothing here answers it, with no feed (`asked_elsewhere`); a receipt's pane says
//! what a run that finished was held to, spent, changed and opened, and under a report the
//! findings that block it, over the harvested field, with no title and no feed (`receipt`);
//! an ended run's pane says, under the title of the node its end names, how it failed, its
//! successor, or where its log stops making sense, with no feed (`ended`); a card's pane
//! holds a decision in place at its foot, under what it rests on, with no feed (`card`); a
//! round's pane holds a round of questions in place under the request it is about, with no
//! title and no feed (`round`); the desk's pane holds the plan's desk in place, which heads
//! itself, with no title and no feed (`desk`).
//!
//! What a node's sessions said is not said here: the log keeps a note's size and digest and
//! no words, so a pane would say it only while a session is heard, and the targets draw it
//! nowhere.

mod asked_elsewhere;
mod board;
mod card;
mod desk;
mod ended;
mod findings;
mod node;
mod pull_request;
mod receipt;
mod round;
mod standstill;

use surco::components::{Card, CardBody, Choices, Legend, Note, Region};
use surco::doc::{Block, Doc};
use surco::look::Look;
use yunta_api::Document;
use yunta_core::events::{ArtifactId, PauseReason};
use yunta_core::{Node, NodeId};
use yunta_render::blocks::{HandsOver, NodeCommand, NodeTitle};
use yunta_render::{chronicle, escalation, state, watched};
use yunta_replay::frame::WaitingOn;

use super::scene::ComposedPane;
use super::{Framed, RunStage};

impl RunStage {
    /// What a person reads of the run `framed` in the pane: the pane its scene composes for
    /// the node pointed at, `look_failed` said in it where a look failed; before the run is
    /// read, which stands as no scene, the look that failed alone.
    pub(super) fn read(
        &self,
        framed: Option<&Framed<'_>>,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let composed = framed.and_then(|framed| {
            let pane = self.scene_of(framed)?.composes();
            Some((pane, framed, self.pointed_in(framed)?))
        });
        match composed {
            Some((pane, framed, pointed)) => {
                self.composed(pane, framed, &pointed, look_failed, look)
            }
            None => {
                let failed = look_failed.map(|failed| Block::from(Note(failed.clone())));
                Region::new(Doc::new().with_all(failed))
            }
        }
    }

    /// The pane `pane` of the run `framed` for the node `pointed`, `look_failed` said in it
    /// where a look failed.
    fn composed(
        &self,
        pane: ComposedPane,
        framed: &Framed<'_>,
        pointed: &NodeId,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        match pane {
            ComposedPane::Work => self.work_pane(framed, pointed, look_failed, look),
            ComposedPane::Board => self.board_pane(framed, pointed, look_failed, look),
            ComposedPane::Findings => self.findings_pane(framed, pointed, look_failed, look),
            ComposedPane::Node => self.node_pane(framed, pointed, look_failed, look),
            ComposedPane::Standstill => self.standstill_pane(framed, pointed, look_failed, look),
            ComposedPane::PullRequest => self.pull_request_pane(framed, pointed, look_failed),
            ComposedPane::AskedElsewhere => self.asked_elsewhere_pane(framed, pointed, look_failed),
            ComposedPane::Receipt => self.receipt_pane(framed, look_failed, look),
            ComposedPane::Ended => self.ended_pane(framed, pointed, look_failed, look),
            ComposedPane::Card(grounds) => {
                self.card_pane(framed, (pointed, grounds), look_failed, look)
            }
            ComposedPane::Round => self.round_pane(framed, look_failed, look),
            ComposedPane::Desk => self.desk_pane(framed, look_failed, look),
        }
    }

    /// The work scene's pane of the run `framed` for the node `pointed`: its title, what
    /// holds its work still, what it runs or what it hands over, `look_failed` said under
    /// them where a look failed, then the node's feed, under its heading where the feed
    /// lists anything. What a node runs says what it works on, so at the least terminal it
    /// stands whole over its feed's newest rows, where what a node hands over gives way.
    fn work_pane(
        &self,
        framed: &Framed<'_>,
        pointed: &NodeId,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let run = &framed.in_view.run;
        let declared = run
            .manifest
            .workflow
            .iter_nodes()
            .find(|declared| declared.id == *pointed);
        let runs = declared
            .and_then(|declared| NodeCommand::of(declared, &run.manifest.config, &run.state));
        let above = || -> Vec<Block> {
            let does = match (&runs, declared) {
                (Some(runs), _) => runs.blocks(),
                (None, Some(declared)) => hands_over(framed, declared, look),
                (None, None) => Vec::new(),
            };
            let failed = look_failed.map(|failed| Block::from(Note(failed.clone())));
            still(framed, look)
                .into_iter()
                .chain(does)
                .chain(failed)
                .collect()
        };
        let abridged = match runs {
            Some(_) => vec![above()],
            None => Vec::new(),
        };
        let doc = Doc::new()
            .with_all(above())
            .with_all(self.feed_under_its_heading(framed, pointed, look));
        titled(Region::new(doc).abridged(abridged), framed, pointed)
    }

    /// Why what the run `framed` waits on a person for could not be read, where it leads
    /// and could not read it for the pause it stands parked on: the pane of its node at work
    /// says it where the card would stand.
    pub(super) fn unread_in_its_pane<'a>(&self, framed: &Framed<'a>) -> Option<&'a str> {
        let in_view = framed.in_view;
        self.leads()
            .then(|| in_view.unreadable_ask(&framed.frame))
            .flatten()
    }

    /// The feed of the node `pointed` of the run `framed`, under its heading, which counts
    /// the open attempt's calls, where the feed lists anything; where what the run waits on
    /// a person for could not be read, the card standing in its place.
    fn feed_under_its_heading(
        &self,
        framed: &Framed<'_>,
        pointed: &NodeId,
        look: &Look,
    ) -> Vec<Block> {
        if let Some(why) = self.unread_in_its_pane(framed) {
            return vec![in_the_cards_place(framed, why)];
        }
        let listed = self.listed(framed, pointed);
        let feed = self.feed_of(framed, pointed, &listed, look).into();
        if listed.is_empty() {
            return vec![feed];
        }
        let calls = chronicle::open_attempt_calls(&listed);
        let heading = Block::Heading(chronicle::feed_heading(calls, look.glyphs));
        vec![heading, feed]
    }
}

/// What the node's work of the run `framed`, standing still, says above it, as render says
/// it, on a row of its own: none where nothing holds it still that it says.
fn still(framed: &Framed<'_>, look: &Look) -> Option<Block> {
    let state = &framed.in_view.run.state;
    let line = watched::still(&framed.frame, state, look.glyphs)?;
    Some(Block::Rows(vec![line]))
}

/// The card the run `framed` waits on a person for, standing where its own would and saying
/// `why` what it asks could not be read: its question, as the escalation it is parked on
/// asks it, else its gist; the node it waits at; then why, a line a part, and nothing to
/// choose.
fn in_the_cards_place(framed: &Framed<'_>, why: &str) -> Block {
    let frame = &framed.frame;
    let question = match frame.phase.waiting_on().and_then(WaitingOn::reason) {
        Some(PauseReason::Escalation(escalation)) => escalation::question(escalation),
        _ => state::gist(frame).unwrap_or_default(),
    };
    let about = why.lines().map(str::trim).filter(|part| !part.is_empty());
    Block::Card(Card {
        question,
        edge: frame.waits_on_a_person_at().map(NodeId::to_string),
        about: about.map(str::to_string).collect(),
        body: CardBody::Choosing {
            choices: Choices {
                rows: Vec::new(),
                at: 0,
            },
            then: None,
        },
        legend: Legend::new(),
    })
}

/// `region`, the pane of the node `pointed` of the run `framed`, under the node's title.
fn titled(region: Region, framed: &Framed<'_>, pointed: &NodeId) -> Region {
    match title_of(framed, pointed) {
        Some(title) => title.titled(region),
        None => region,
    }
}

/// The title of the pane of the node `pointed` of the run `framed`: none for a node the run
/// does not hold.
fn title_of(framed: &Framed<'_>, pointed: &NodeId) -> Option<NodeTitle> {
    let run = &framed.in_view.run;
    let declared = run
        .manifest
        .workflow
        .iter_nodes()
        .find(|declared| declared.id == *pointed);
    let node = framed.frame.nodes.iter().find(|node| node.id == *pointed)?;
    Some(NodeTitle::of(
        node,
        declared,
        &run.state,
        &run.manifest.config,
    ))
}

/// What `declared`'s node of the run `framed` hands over: each artifact it declares, where it
/// stands on the node's open attempt, one the follower could not read said with why.
fn hands_over(framed: &Framed<'_>, declared: &Node, look: &Look) -> Vec<Block> {
    let produces = declared
        .artifacts
        .as_ref()
        .map(|artifacts| artifacts.produces.as_slice())
        .unwrap_or_default();
    let unread: Vec<(ArtifactId, String)> = framed
        .in_view
        .handed_over
        .get(&declared.id)
        .into_iter()
        .flatten()
        .filter_map(|document| match document {
            Document::Unread { shown, why, .. } => Some((shown.artifact.clone(), why.clone())),
            Document::Read(_) => None,
        })
        .collect();
    HandsOver::of(&declared.id, produces, &framed.in_view.run.state, &unread).blocks(look)
}
