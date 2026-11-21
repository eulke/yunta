//! See [`crate`]. What the state keeps of what it follows, folded from what the
//! performer's one following hands: the field as far as it reaches, each run of it at its
//! head; and the run in view, read whole, with its moments indexed by node, what its
//! sessions said, what it waits on a person for, what its nodes handed over and what
//! reading it remarks on. It is kept for every stage to read, so a stage opened over a
//! run's sees the run move under it.
//!
//! Beside it stand where the project keeps its runs and what it prices tokens at, as the
//! repository's reading said them: neither is followed, and a stage reads them from the
//! same value as what is.
//!
//! What it keeps sits behind `Arc`, each run of the field and the run in view, and is
//! changed by `Arc::make_mut`: the clone of a state a walk makes for each input shares
//! them until one of the two changes them.
//!
//! The fold takes an event once, as replay's fold does: an event at or below a run's head
//! moves nothing, a run handed whole again where it is held at that head or past it
//! changes nothing, and every other piece takes the place of what it repeats, so news
//! handed again change nothing. Each arm of `FollowNews` and `RunNews` is matched by name,
//! with no wildcard: an arm added to either does not compile here until it folds.
//!
//! What the run in view waits on is kept with the pause it was read for, so a reading
//! for a pause the run left is never taken for one of the pause it stands at.
//!
//! What a node's sessions said is let go once the node stops working, as the run's rows
//! say it, so a node working again shows none of its last session's words; and words said
//! before their node last started or ended are not heard again when they are handed again,
//! nor their missed words counted again.
//!
//! A remark waits with the run in view, kept once, for the run's stage to say once as a
//! note; the field keeps none, since no row says a run's remarks.
//!
//! Every stage that draws the design's frame frames itself through what is kept here, by
//! `FollowedRuns::framing` (`framing`): the tally of the field, and the attention row,
//! from where the person stands. Where a line is typed every key types, `a` among them, so
//! the row says what needs the person and names no key there.
//!
//! It keeps only what is followed: news of a field or a run the state does not follow is
//! dropped, and what the state no longer follows is let go as it asks for the next
//! following, as the following lets it go. The field followed at another reach is the one
//! thing kept a while: it stands until the field at the new reach comes whole in its
//! place, so the field's stage shows the runs it last read meanwhile. A promotion's
//! successor handed of the run in view is read whole and kept beside it, what comes to it
//! folded there, and the fold says so, for the state to move every stage that shows the run
//! onto it where the run leads; the successor takes the run's place once the state follows
//! it. A look that failed is
//! kept until the next news, so what was read before is said to be as old as the last
//! look that read.
//!
//! A run this window handed to a detached engine itself is kept as handed on, from the
//! result of the work that handed it on (a start, a resume, a decision or a round's answers
//! recorded) until a reading of its engine says anything but that its registry is not
//! written: until then the engine it was handed to is about to write it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_api::{
    AnswerablePause, Awaited, Document, FieldReach, FieldReading, FollowNews, Followed,
    FollowedField, FollowedRun, Remark, RunNews,
};
use yunta_core::events::StoredEvent;
use yunta_core::{NodeId, PricingEntry, RunId, Seq};
use yunta_render::page::{FieldRow, UnreadableRun};
use yunta_render::watched::Heard;
use yunta_replay::frame::Moment;
use yunta_replay::registry::Engine;

use crate::Request;

mod framing;
mod in_view;

pub(crate) use framing::{waiting_longest, Attended, ItsDecision, Standpoint};

/// What the state follows, as it last asked for it, and what it keeps of it. Not
/// `yunta-api`'s `FollowedRun`, one run as the follower hands it.
#[derive(Debug, Clone)]
pub struct FollowedRuns {
    /// The field, once read whole at the reach followed, or at the reach followed before
    /// until the field at this one comes.
    pub field: Option<FieldInView>,
    /// The run followed, once read whole, or the one a promotion handed its work to.
    pub in_view: Option<Arc<RunInView>>,
    /// What the last look said where it failed whole: what is kept stands as old as the
    /// last look that read. None once news comes again.
    pub look_failed: Option<String>,
    /// The store the project's runs log their events in, once the repository is read.
    pub storage: Option<PathBuf>,
    /// What the project prices each model's tokens at, where its configuration says,
    /// once the repository is read.
    pub prices: Option<BTreeMap<String, PricingEntry>>,
    /// What the state last asked to follow.
    asked: Followed,
    /// The runs this window handed to a detached engine itself, each from the result of the
    /// work that handed it on until the first reading of its engine that is not
    /// `Engine::Unrecorded`: until then the engine it was handed to is about to write its
    /// registry. Not a run's engine as read, which the run in view and the field keep.
    handed_on: BTreeSet<RunId>,
}

/// The field as the state keeps it: each run of it by id, at its head. Not `yunta-api`'s
/// `FollowedField`, the field as the follower hands it whole.
#[derive(Debug, Clone)]
pub struct FieldInView {
    /// How far it reaches: the reach it was asked at.
    pub reach: FieldReach,
    /// Each run of the field that reads, at its head.
    pub runs: BTreeMap<RunId, Arc<FollowedRun>>,
    /// The runs of the field it can name but not read, with why.
    pub unreadable: Vec<UnreadableRun>,
    /// How many runs the log holds that it leaves to other repositories.
    pub elsewhere: usize,
    /// Whether it is the runs of the repository it was asked in, rather than every run on
    /// this machine.
    pub here: bool,
}

/// The run in view as the state keeps it: read whole, and all that came to it since.
#[derive(Debug, Clone)]
pub struct RunInView {
    /// The run, at its head.
    pub run: FollowedRun,
    /// What happened to it, in order: the chronicle of the events it was read whole with,
    /// then the moments each batch folded in hands back.
    pub moments: Vec<Moment>,
    /// Where in `moments` each node's moments stand, in order.
    pub by_node: BTreeMap<NodeId, Vec<usize>>,
    /// What each node handed over, as the follower read it: read, or unread with why.
    pub handed_over: BTreeMap<NodeId, Vec<Document>>,
    /// What its sessions said last, a frame to each node.
    pub heard: Heard,
    /// How many words its sessions said that the following missed for falling behind.
    pub missed: u64,
    /// What it waits on a person for, as the follower read it for a pause; none while it
    /// waits on none.
    pub awaited: Option<AwaitedReading>,
    /// What reading it remarks on, each once, which its stage says once.
    pub remarks: Vec<Remark>,
    /// The run a promotion handed its work to, read whole and kept beside it, with what
    /// came to it since: none before one comes. It takes the place of this one where the
    /// state follows it.
    pub successor: Option<Box<RunInView>>,
    /// Whether it closed promoted and no successor came in the time one takes to be
    /// created.
    pub no_successor: bool,
}

/// What the run in view waits on a person for, as the follower read it for `pause`. Not
/// `yunta-api`'s `Awaited`, which only a read that succeeded hands.
#[derive(Debug, Clone)]
pub enum AwaitedReading {
    /// Read: the decision or the round of questions a person answers.
    Read {
        pause: AnswerablePause,
        awaited: Awaited,
    },
    /// Not read, for `why`: a document its decision shows fails today's shape, or its
    /// round's document cannot be read. The run still waits.
    Unread { pause: AnswerablePause, why: String },
}

impl AwaitedReading {
    /// The pause it was read for.
    pub fn pause(&self) -> &AnswerablePause {
        match self {
            AwaitedReading::Read { pause, .. } | AwaitedReading::Unread { pause, .. } => pause,
        }
    }
}

impl FollowedRuns {
    /// Nothing followed, and nothing kept.
    pub(crate) fn new() -> FollowedRuns {
        FollowedRuns {
            field: None,
            in_view: None,
            look_failed: None,
            storage: None,
            prices: None,
            asked: Followed {
                field: None,
                run: None,
            },
            handed_on: BTreeSet::new(),
        }
    }

    /// `run`, handed from this window to a detached engine, started, resumed or answered,
    /// kept as handed on until a reading of its engine names more than an unwritten registry.
    pub(crate) fn handed_on(&mut self, run: RunId) {
        self.handed_on.insert(run);
        self.engines_read();
    }

    /// Whether this window handed `run` to a detached engine whose registry no reading has
    /// shown written yet: such a run's unrecorded engine is one about to write it.
    pub(crate) fn handed_on_unread(&self, run: &RunId) -> bool {
        self.handed_on.contains(run)
    }

    /// The runs handed on here whose engine a reading kept here names, alive, dead or
    /// unknown, let go: from then on the reading alone says whether an engine drives it.
    fn engines_read(&mut self) {
        let read = |run: &RunId| {
            let in_view = self.in_view.as_deref();
            let in_view = in_view.filter(|in_view| in_view.run.run == *run);
            let in_field = self.field.as_ref().and_then(|field| field.runs.get(run));
            let engines = in_view.map(|in_view| &in_view.run.engine);
            let engines = engines.into_iter().chain(in_field.map(|run| &run.engine));
            engines
                .into_iter()
                .any(|engine| *engine != Engine::Unrecorded)
        };
        let handed_on = std::mem::take(&mut self.handed_on);
        let unread = handed_on.into_iter().filter(|run| !read(run)).collect();
        self.handed_on = unread;
    }

    /// The request to follow `followed`, where it is not what was last asked; what is no
    /// longer followed let go, as the following lets it go: the field once none is
    /// followed, and the run in view once another run or none is. The field at another
    /// reach stands until the field at this one comes whole in its place.
    pub(crate) fn asked(&mut self, followed: Followed) -> Option<Request> {
        if followed == self.asked {
            return None;
        }
        if followed.field.is_none() {
            self.field = None;
        }
        if followed.run != self.asked.run {
            let kept = self.in_view.take();
            self.in_view = kept.and_then(|kept| in_place_of(kept, followed.run.as_ref()));
        }
        self.asked = followed.clone();
        Some(Request::Follow(followed))
    }

    /// `news` folded into what is kept of what it is about, where that is followed; the
    /// look that failed before it cleared. Where the news hands the work of the run in view
    /// to a successor, which takes its place, that run and its successor.
    pub(crate) fn took(&mut self, news: FollowNews) -> Option<(RunId, RunId)> {
        let turned = self.folded(news);
        self.engines_read();
        turned
    }

    /// `news` folded into what is kept of what it is about, as [`FollowedRuns::took`] says.
    fn folded(&mut self, news: FollowNews) -> Option<(RunId, RunId)> {
        self.look_failed = None;
        match news {
            FollowNews::Run { run, news } => return self.run_took(&run, news),
            FollowNews::Field(field) => {
                self.field = self
                    .asked
                    .field
                    .map(|reach| FieldInView::read(*field, reach));
            }
            FollowNews::Joined(run) => self.in_the_field(|field| field.joined(*run)),
            FollowNews::Unreadable(why) => self.in_the_field(|field| field.unreadable(why)),
            FollowNews::Left(run) => self.in_the_field(|field| field.left(&run)),
            FollowNews::Elsewhere(elsewhere) => {
                self.in_the_field(|field| field.elsewhere = elsewhere);
            }
            FollowNews::Whole { run, events } => self.read_whole(*run, &events),
        }
        None
    }

    /// Whether what `followed` names is read at `heads`, each run's last seq in the log,
    /// at `now`: the field read whole at its reach, with every run of the log in it and
    /// each run it reads at its head; the run read whole at its head, what its nodes handed
    /// over by then kept, where it stands
    /// parked on a person with a menu or a round, what it waits for read for that pause,
    /// and where it closed promoted, its successor in its place or none come.
    pub(crate) fn caught_up(
        &self,
        followed: &Followed,
        heads: &BTreeMap<RunId, Seq>,
        now: DateTime<Utc>,
    ) -> bool {
        let field = match (followed.field, &self.field) {
            (None, _) => true,
            (Some(reach), Some(field)) => field.reach == reach && field.caught_up(heads),
            (Some(_), None) => false,
        };
        let run = match (&followed.run, &self.in_view) {
            (None, _) => true,
            (Some(run), Some(in_view)) => in_view.run.run == *run && in_view.caught_up(heads, now),
            (Some(_), None) => false,
        };
        field && run
    }

    /// `change` made to the field, where one is kept.
    fn in_the_field(&mut self, change: impl FnOnce(&mut FieldInView)) {
        if let Some(field) = self.field.as_mut() {
            change(field);
        }
    }

    /// `run`, read whole with `events`, made the run in view where it is the run
    /// followed: one held at that head or past it already changes nothing.
    fn read_whole(&mut self, run: FollowedRun, events: &[StoredEvent]) {
        if self.asked.run.as_ref() != Some(&run.run) {
            return;
        }
        let held = self.in_view.as_ref().filter(|held| held.run.run == run.run);
        if held.is_some_and(|held| held.run.state.head() >= run.state.head()) {
            return;
        }
        self.in_view = Some(Arc::new(RunInView::read_whole(run, events)));
    }

    /// `news` of `run` folded into each copy kept under that id: its row in the field, and
    /// the run in view or the successor kept beside it. Where a successor of the run in view
    /// came, `run` and that successor.
    fn run_took(&mut self, run: &RunId, news: RunNews) -> Option<(RunId, RunId)> {
        if let Some(field) = self.field.as_mut() {
            field.took(run, &news);
        }
        let in_view = Arc::make_mut(self.in_view.as_mut()?);
        let succeeded = matches!(news, RunNews::Successor { .. });
        let taken = match in_view.run.run == *run {
            true => in_view,
            false => in_view
                .successor
                .as_deref_mut()
                .filter(|successor| successor.run.run == *run)?,
        };
        taken.took(news);
        let by = taken.successor.as_ref().filter(|_| succeeded)?;
        Some((run.clone(), by.run.run.clone()))
    }
}

/// `kept`, the run in view, where `followed` is the run it is, or the successor it keeps
/// beside it where `followed` is that one: what the run in view is once the state follows
/// `followed`. None where it is neither.
fn in_place_of(kept: Arc<RunInView>, followed: Option<&RunId>) -> Option<Arc<RunInView>> {
    let followed = followed?;
    if kept.run.run == *followed {
        return Some(kept);
    }
    let successor = kept.successor.as_deref()?;
    (successor.run.run == *followed).then(|| Arc::new(successor.clone()))
}

impl FieldInView {
    /// The field `field` hands, read at `reach`.
    fn read(field: FollowedField, reach: FieldReach) -> FieldInView {
        let runs = field.runs.into_iter();
        FieldInView {
            reach,
            runs: runs.map(|run| (run.run.clone(), Arc::new(run))).collect(),
            unreadable: field.unreadable,
            elsewhere: field.elsewhere,
            here: field.here,
        }
    }

    /// Its rows at `now`, each run's made the one way `yunta list --runs` makes it.
    pub fn rows(&self, now: DateTime<Utc>) -> Vec<FieldRow> {
        self.runs.values().map(|run| run.row(now)).collect()
    }

    /// Whether it holds a run, read or not: a repository set up whose field holds none
    /// asks for its first run in the field's place.
    pub(crate) fn holds_a_run(&self) -> bool {
        !self.runs.is_empty() || !self.unreadable.is_empty()
    }

    /// It as the field's stage reads a field at `now`: its rows, the runs it cannot read,
    /// how many it leaves elsewhere and whether it is here.
    pub(crate) fn reading(&self, now: DateTime<Utc>) -> FieldReading {
        FieldReading {
            rows: self.rows(now),
            unreadable: self.unreadable.clone(),
            elsewhere: self.elsewhere,
            here: self.here,
        }
    }

    /// `run` joined the field at its head, out of those it cannot read: one it holds at
    /// that head or past it already changes nothing.
    fn joined(&mut self, run: FollowedRun) {
        self.unreadable.retain(|unread| unread.run_id != run.run);
        let held = self.runs.get(&run.run);
        if held.is_some_and(|held| held.state.head() >= run.state.head()) {
            return;
        }
        self.runs.insert(run.run.clone(), Arc::new(run));
    }

    /// A run of the field unreadable for `why`, in place of what was held of it.
    fn unreadable(&mut self, why: UnreadableRun) {
        self.runs.remove(&why.run_id);
        match self
            .unreadable
            .iter_mut()
            .find(|unread| unread.run_id == why.run_id)
        {
            Some(unread) => *unread = why,
            None => self.unreadable.push(why),
        }
    }

    /// A run gone from the log, out of the field.
    fn left(&mut self, run: &RunId) {
        self.runs.remove(run);
        self.unreadable.retain(|unread| unread.run_id != *run);
    }

    /// `news` of `run` folded into its row, where the field reads it: what came to its log
    /// and to its engine. The rest is the run in view's alone, a successor among it, which
    /// joins the field as any run new to the log does.
    fn took(&mut self, run: &RunId, news: &RunNews) {
        let Some(held) = self.runs.get_mut(run) else {
            return;
        };
        match news {
            RunNews::Events(_) | RunNews::Engine(_) => {
                Arc::make_mut(held).took(news);
            }
            RunNews::Said { .. }
            | RunNews::Successor { .. }
            | RunNews::NoSuccessor
            | RunNews::Awaited(_)
            | RunNews::AwaitedUnread { .. }
            | RunNews::Document { .. }
            | RunNews::Remark(_) => {}
        }
    }

    /// Whether every run of the log at `heads` is in it, read, unreadable or counted
    /// elsewhere, and each run it reads stands at its head there.
    fn caught_up(&self, heads: &BTreeMap<RunId, Seq>) -> bool {
        let every = self.runs.len() + self.unreadable.len() + self.elsewhere == heads.len();
        let at_heads = self
            .runs
            .iter()
            .all(|(id, run)| run.state.head() == heads.get(id).copied());
        every && at_heads
    }
}
