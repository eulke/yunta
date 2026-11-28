//! See [`super`]. A run, followed: its headline, its ribbon and its gist across the screen,
//! its nodes beside its pane as a list the pointer moves along, the pane its scene composes
//! for the node pointed at, and under them what it has spent.
//!
//! The stage keeps none of the run: what stands across, its nodes, its meter and its
//! chronicle are the run in view's, which the state keeps of what it follows, drawn where it
//! is the stage's run from the frame the run in view keeps for its head, what the clock adds
//! to it worked out at the instant of each draw. They move as the run's news come, a node
//! pointed at or not, and its ages move with the clock. The moments are the ones
//! the state's fold tells, so the chronicle and the ribbon are the ones `status --follow`
//! draws from the same log; whether the run offers a menu is the run in view's, worked out
//! once for its head, and so is the decision its acts are chosen by: what the run in view
//! waits on, once read for the pause it stands at. A look that failed is said in the pane,
//! over the run as it was last read, until the next news.
//!
//! The stage asks for nothing as it opens: the state follows the stage's run under every
//! stage over it, and the stage draws what the state keeps. Where the work goes on in a
//! promotion's successor while the run leads, the state moves the stage onto it as it folds
//! the successor (`succeeded`), its pane from its first line. Where the person holds the
//! screen, the run stays, until `f` gives it the lead back and the successor takes its place
//! (`successor`). Where the run leads parked on a child it runs, the stage moves onto the
//! child, the run kept above it for the trail and `esc` to go up to (`child`).
//!
//! The main area stands as the run's scene (`scene`), whose pane it draws (`pane`): none
//! before the run is read. The stage opens on the run's decision as its command shows it
//! ([`Opens`]), on its card for `status` and `resolve-gate`, on the plan for `review`: for
//! the first prompt it holds, and no later one.
//!
//! A pointer moves along the run's nodes and is kept by the node it stands on; where the
//! run no longer holds that node, its first node stands for it. The run moves it while it
//! leads the screen, a person once they take it, and `f` gives it back (`lead`); the trail
//! names the node it stands on. Where the pane lists the run's findings, `j k` move a
//! second pointer along them, until `tab` hands them to the nodes (`lead`). The node a
//! person points at shows its own scene in the pane, letting nothing go, while the rows go
//! on moving. The keys a run binds, and the acts the run offers that a key here takes,
//! are `acts`': the legend says each it names in render's words, `f` among them while the
//! run does not lead.
//!
//! The screen is laid out from the run in view (`screen`) and stands in the design's frame
//! (`frame`): the tally and the attention row from where the person stands on the run, the
//! legend with the far keys, and the note, which says what refused or failed and what
//! reading the run remarked on until the next key. Where the run leads parked on a person,
//! the prompt held for that pause stands in place, its keys its own (a card, a round, the
//! plan's desk), until `esc` puts it aside, `a` or `f` bringing it back, or its pause goes,
//! a note saying who answered it elsewhere (`in_place`).
//!
//! Where the run waits on a pull request, the stage polls its forge once for the pause its
//! scene stands at, and keeps what the forge answered beside that pause; `o` shows the pull
//! request by the link its log holds (`pull_request`). Where it finished, the stage has its
//! store verify its event chain once for the run, as its receipt first shows, and derives the
//! receipt at each draw from the run in view (`receipt`). Each is what its scene reads
//! beyond the log, read once by one rule (`read_once`); what each work the stage asked for
//! came to is `answered`'s.
//!
//! The pane scrolls by the arrows and the page keys, which leave the pointer where it
//! stands; it keeps its line as the run moves, and starts at its first when it is another
//! node's, the run's again once the lead is given back, or a successor's. A pane that ends
//! with a node's feed scrolls its feed alone, which opens at its end and is the run's there,
//! one more thing the person takes by scrolling it back (`feed`).

mod acts;
mod answered;
mod child;
mod feed;
mod frame;
mod in_place;
mod lead;
mod pane;
mod pull_request;
mod read_once;
mod receipt;
mod scene;
pub(super) mod screen;
mod successor;

pub use scene::{Grounds, Scene, Standstill};

use chrono::{DateTime, Utc};
use surco::components::Laid;
use yunta_api::{PullRequestPolled, Remark};
use yunta_core::receipt::EventChainStatus;
use yunta_core::{FindingId, NodeId, RunId, Seq};
use yunta_render::screen::Along;
use yunta_replay::frame::{RunFrame, RunTimes};

use super::{Act, Want};
use crate::followed_runs::Attended;
use crate::prompt::{Opens, Stroke};
use crate::scroll::{Scroll, ScrolledTo};
use crate::{FollowedRuns, RunInView};
use frame::Notice;
use in_place::InPlace;
use lead::PointedBy;
use read_once::ReadOnce;

/// A run a person follows: where they stand on it. Not the run, which the state keeps of
/// what it follows.
#[derive(Debug, Clone)]
pub struct RunStage {
    /// The run followed: the one opened, the last a promotion handed the work to, or the
    /// child shown in the place of the run that bore it.
    pub(super) run: RunId,
    /// The runs whose child the stage shows in their place, outermost first: none where it
    /// shows the run opened or its successor.
    parents: Vec<child::Parent>,
    /// What the first prompt held for the run's decision opens on, as the command that opened
    /// the stage asks: its card, or the plan it is on, for a person who came to read the
    /// plan. Taken by that prompt: a later pause stands as its scene says.
    opens: Option<Opens>,
    /// The node a key acts on and the trail names: where the run put it, or where a person
    /// moved it. Where the run does not hold it, its first node stands for it.
    node: PointedBy<NodeId>,
    /// The finding the run's findings are read at: by the node that posted it, none for
    /// one the engine posted about the run, and its id among that node's.
    finding: PointedBy<(Option<NodeId>, FindingId)>,
    /// What `j k` move where the pane lists the run's findings: the findings, until `tab`
    /// hands them to the nodes and back.
    along: Along,
    /// Where the pane is scrolled to, where it scrolls by itself: not a pane that ends with
    /// a feed, nor a prompt in place, each of which keeps its own.
    scroll: Scroll,
    /// Where the feed the pane ends with is scrolled to: its end, until a person scrolls it
    /// back.
    feed: Scroll,
    /// What the screen notes until the next key: what the last piece of work came to where
    /// it did not do what was asked, or what reading the run remarked on.
    notice: Option<Notice>,
    /// What reading the run remarked on that the stage has said, each once.
    remarked: Vec<Remark>,
    /// The pull request of the gate the run waits at, read for the pause its scene stood
    /// at, and what the forge answered, where the run waited at a gate it holds by then.
    pull_request: ReadOnce<Seq, Option<Box<PullRequestPolled>>>,
    /// The event chain of the run whose receipt the scene shows, read for the run, and what
    /// its store's verification found, or why it could not.
    chain: ReadOnce<RunId, Result<EventChainStatus, String>>,
    /// The prompt held for the pause the run waits on a person at, where its scene holds it
    /// in place: built once for that pause (`in_place`). Boxed: kept in place it would make
    /// every stage the state holds as large as this one.
    in_place: Option<Box<InPlace>>,
}

/// The run in view, framed at an instant: what the stage draws and acts on.
struct Framed<'a> {
    in_view: &'a RunInView,
    /// Its frame as the run in view keeps it, derived once for its head.
    frame: &'a RunFrame,
    /// What the clock adds to its frame at `now`.
    times: RunTimes,
    /// The instant it is framed at, which what is read beside it is framed at too.
    now: DateTime<Utc>,
    /// Whether this window handed the run to a detached engine whose registry no reading
    /// has shown written yet: an engine is about to write it.
    handed_on: bool,
}

impl RunStage {
    /// The stage of `run`, before anything of it is read, its decision opening on `opens`.
    pub(crate) fn opened_on(run: RunId, opens: Opens) -> Self {
        RunStage {
            run,
            parents: Vec::new(),
            opens: Some(opens),
            node: PointedBy::TheRun(None),
            finding: PointedBy::TheRun(None),
            along: Along::Findings,
            scroll: Scroll::default(),
            feed: Scroll::at_the_end(),
            notice: None,
            remarked: Vec::new(),
            pull_request: ReadOnce::default(),
            chain: ReadOnce::default(),
            in_place: None,
        }
    }

    /// What the stage asks for when a person comes back to it: its pointers where the run
    /// stands, where it leads, and the run it shows where that moved; the prompt held for
    /// the pause the run stands at, where it stands in place; a remark the run came to
    /// meanwhile, noted; what its scene reads beyond the log, where it stands for what it was
    /// not read for: a reading under way as the person left stays its own, answered under the
    /// stage over it.
    pub(crate) fn returned(&mut self, followed: &FollowedRuns) -> Vec<Want> {
        self.placed(followed);
        let framed = self.framed_at_its_head(followed);
        self.prompt_placed(framed.as_ref());
        self.remark_taken(followed);
        self.read_once(framed.as_ref())
    }

    /// What the stage asks once the state folded a piece of what it follows into
    /// `followed`: its pointers where the run stands, where it leads, and the run it shows
    /// where that moved; the prompt held for the pause the run stands at, where it stands in
    /// place; a remark noted, and what its scene reads beyond the log, where it stands for
    /// what it was not read for.
    pub(crate) fn followed_on(&mut self, followed: &FollowedRuns) -> Vec<Want> {
        self.placed(followed);
        let framed = self.framed_at_its_head(followed);
        self.prompt_placed(framed.as_ref());
        self.remark_taken(followed);
        self.read_once(framed.as_ref())
    }

    /// Both pointers put where the run `followed` keeps stands, where it leads, and the run
    /// the stage shows moved where that run's place says: up to a parent the field shows no
    /// longer parked on the run under it, onto the child the run stands parked on, or onto
    /// the successor its promotion handed the work to.
    fn placed(&mut self, followed: &FollowedRuns) {
        self.up_where_a_parent_moved(followed);
        if let Some(framed) = self.framed_at_its_head(followed) {
            self.led(&framed);
            self.onto_its_child(&framed);
            self.onto_its_successor(&framed);
        }
    }

    /// The run in view `followed` keeps, framed at `now`, where it is this stage's run:
    /// none until it is read. Its frame is the one the run in view keeps; only what the clock
    /// adds is worked out here.
    fn framed<'a>(&self, followed: &'a FollowedRuns, now: DateTime<Utc>) -> Option<Framed<'a>> {
        let in_view = followed.in_view.as_deref()?;
        (*in_view.run.run() == self.run).then(|| Framed {
            frame: in_view.run.frame(),
            times: in_view.run.times(now),
            now,
            in_view,
            handed_on: followed.handed_on_unread(&self.run),
        })
    }

    /// The run in view `followed` keeps, framed at the instant of its last event, where it is
    /// this stage's run: where the run stands, which its pointers follow, reads no clock.
    fn framed_at_its_head<'a>(&self, followed: &'a FollowedRuns) -> Option<Framed<'a>> {
        let in_view = followed.in_view.as_deref()?;
        let head = in_view.run.state().clock().last_at()?;
        self.framed(followed, head)
    }

    /// Whether the run leads its screen: both its pointers are its own and the feed its pane
    /// ends with stands at its end. Derived each time, never kept.
    pub(crate) fn leads(&self) -> bool {
        let fed_to_its_end = self.feed.to() == ScrolledTo::End;
        self.node.is_the_runs() && self.finding.is_the_runs() && fed_to_its_end
    }

    /// Both pointers put where the run `framed` stands, where it leads: while a person
    /// holds either, the run's news move neither.
    fn led(&mut self, framed: &Framed<'_>) {
        if !self.leads() {
            return;
        }
        let state = framed.in_view.run.state();
        let node = lead::node_led(framed.frame, state, self.node.on());
        self.node = PointedBy::TheRun(node);
        self.finding = PointedBy::TheRun(lead::finding_led(&state.findings));
    }

    /// The node a key acts on among the nodes of the run `framed`: the one pointed at where
    /// the run holds it, where the run would put it before it put it anywhere, its first
    /// node otherwise.
    fn pointed_in(&self, framed: &Framed<'_>) -> Option<NodeId> {
        let frame = framed.frame;
        let held = |node: &NodeId| frame.nodes.iter().any(|framed| framed.id == *node);
        let pointed = match &self.node {
            PointedBy::TheRun(None) => lead::node_led(frame, framed.in_view.run.state(), None),
            PointedBy::TheRun(Some(node)) | PointedBy::ThePerson(node) => Some(node.clone()),
        };
        pointed
            .filter(held)
            .or(frame.nodes.first().map(|node| node.id.clone()))
    }

    /// What `stroke` asks for here, on the run `followed` keeps at `now`: what the prompt in
    /// place makes of a stroke of its own, where one stands; else moving along the nodes,
    /// the lead given back to the run, going where the attention row says, an act the run
    /// offers, the prompt in place put aside, going up to the run whose child the stage shows,
    /// going to the field, leaving, or scrolling the pane beside; the prompt the scene it
    /// comes to stand as holds in place, and what that scene reads beyond the log, where it
    /// was not read for that. The note it stood on goes, and the next remark not yet said
    /// takes its place.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        self.noted_away(followed);
        let framed = self.framed(followed, now);
        let wants = self.stroked(stroke, framed.as_ref(), followed, now);
        self.prompt_placed(framed.as_ref());
        let read_once = self.read_once(framed.as_ref());
        wants.into_iter().chain(read_once).collect()
    }

    /// What `stroke` asks for on the run `framed`, as `followed` keeps it at `now`, as
    /// [`RunStage::pressed`] says.
    fn stroked(
        &mut self,
        stroke: Stroke,
        framed: Option<&Framed<'_>>,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        if let Some(wants) = self.answered_in_place(stroke, framed) {
            return wants;
        }
        match stroke {
            Stroke::Insert('j') => self.along(1, framed),
            Stroke::Insert('k') => self.along(-1, framed),
            Stroke::Tab => self.tabbed(framed),
            Stroke::Insert('f') => self.given_back(framed),
            Stroke::Insert('a') => self.attending(framed, followed, now),
            Stroke::Insert('q') => acts::left(framed),
            Stroke::Decline => match self.put_aside(framed) {
                Some(wants) => wants,
                None if self.up_to_its_parent() => Vec::new(),
                None => acts::left(framed),
            },
            Stroke::Interrupt => vec![Want::Leave],
            Stroke::Insert(key) => {
                let act = self
                    .offered_in(framed, followed.can_ask_for_a_run())
                    .into_iter()
                    .find(|act| act.key == Some(key));
                match act {
                    Some(act) => acts::took(act.takes),
                    None => {
                        self.pane_in_mut(framed).pressed(stroke);
                        Vec::new()
                    }
                }
            }
            other => {
                self.pane_in_mut(framed).pressed(other);
                Vec::new()
            }
        }
    }

    /// What a surface laid out of the pane, of the run `followed` keeps at `now`, which the
    /// keys that scroll move from: of its feed where it ends with one, of the pane elsewhere.
    pub(crate) fn laid(&mut self, laid: Laid, followed: &FollowedRuns, now: DateTime<Utc>) {
        let framed = self.framed(followed, now);
        self.pane_in_mut(framed.as_ref()).laid(laid);
    }

    /// The stage, where its run is `run`, moved onto `by`, the successor a promotion
    /// handed the work to: the successor's own pane, from its first line, and what reading
    /// the successor remarks on is its own to say.
    pub(crate) fn succeeded(&mut self, run: &RunId, by: &RunId) {
        if self.run != *run {
            return;
        }
        self.run = by.clone();
        self.remarked.clear();
        self.scroll.rewound();
        self.feed.back_at_its_end();
    }

    /// What the run `followed` keeps offers at `now`, each with the key that takes it
    /// here: answering what it waits on first, where `a` goes to the run's own decision,
    /// then every act it offers that takes a step, each said in its own words where it has
    /// them.
    pub(crate) fn offered(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Act> {
        let framed = self.framed(followed, now);
        let its_own = self.attended(framed.as_ref(), followed, now) == Some(Attended::ItsOwn);
        let answering = self.answering(framed.as_ref()).filter(|_| its_own);
        let answering = answering.map(|step| Act::by(Stroke::Insert('a'), step));
        let acts = self
            .offered_in(framed.as_ref(), followed.can_ask_for_a_run())
            .into_iter()
            .filter_map(|act| {
                let acts::Takes::Step(step) = act.takes else {
                    return None;
                };
                let taken = match act.key {
                    Some(key) => Act::by(Stroke::Insert(key), step),
                    None => Act::unbound(step),
                };
                Some(match act.does {
                    Some(does) => taken.saying(does),
                    None => taken,
                })
            });
        answering.into_iter().chain(acts).collect()
    }

    /// Each act the run `framed` offers that a key here takes, by the scene it stands as:
    /// making room where it stands still for want of it, what it offers by its standing and
    /// the decision it waits on, once the state read it for the pause the run stands at, and
    /// a new run where no prompt stands in its scene; before the run is read, where no prompt
    /// stands yet, a new run alone; a new run only where one `asks_for_a_run` here.
    fn offered_in(&self, framed: Option<&Framed<'_>>, asks_for_a_run: bool) -> Vec<acts::Offered> {
        let Some(framed) = framed else {
            return asks_for_a_run.then(acts::a_new_run).into_iter().collect();
        };
        let decision = framed
            .in_view
            .awaited_decision()
            .map(|decision| &decision.escalation);
        let scene = self.scene_of(framed);
        acts::offered(framed, decision, scene.as_ref(), asks_for_a_run)
    }
}
