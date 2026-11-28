//! See [`super`]. The field: the runs of this repository, or of this machine, as
//! render lists them for a screen ([`FieldRows`]), a pointer on the one a key acts on and
//! the runs a person picked for an act that takes several.
//!
//! The stage keeps none of the runs: it is drawn from the field the state keeps of what
//! it follows, framed at the instant of each draw, so a run moves to its group as its news
//! come, and its age moves with the clock. Its screen: the list in the main region, the row
//! pointed at on the selection to the region's edge; the run pointed at, at a glance in the
//! region beside it at the right (`glance`), with what a person can do about it, its acts as
//! the follower keeps them for the run's head, which the list holds under its own where the
//! glance gives way; the trail naming the repository's path as the repository's reading gave
//! it, or this machine where the field lists every run on it; the tally and the attention row as
//! `FollowedRuns::framing` composes them for a screen that shows no run, so a run asked at
//! its own terminal is counted and listed under `asking now`, never named in the attention
//! row; the note, in the attention row's place until the next key, saying what an act here
//! came to or what leaving a run for the field said of it, news leaving it standing; and the
//! legend render words, with the far keys beside it where no line is typed: the keys that act
//! on a run stand in it where a run stands in sight or may yet, before the field is read and
//! while a run started here is on its way into it. A look that failed is said under the list
//! as it was last read, until the next news. Widening the reach follows the machine's field,
//! which comes whole in place of the field shown. A field
//! that holds no run, as `esc` finds it from the first request of a repository with none,
//! lists its first row alone, with no glance beside it; its legend names a new run and
//! quitting, and the keys that act on a run, which act on none there, stand in help alone, as
//! on a field whose runs are closed and hidden.
//!
//! The pointer moves along the runs in the order the list holds them, which render
//! says ([`FieldRows::listed`]), and is kept by the run it stands on, so it stays on a run
//! that moves; where its run left the field, the first run listed stands for it, and a
//! pick, kept by its run too, goes where its run left. A key acts on a run only where the
//! acts the follower keeps for it hold the act the key takes (the glance's one table of
//! keys), so it acts on none the field cannot read: resuming acts at once and notes what it
//! did, and on a run that waits on a decision, which no resume answers, notes that `enter`
//! reads it; stopping asks a confirmation first; closing asks one confirmation for the runs
//! picked, or the one pointed at where none is, naming in its question each run it closes
//! and under its sentence each it leaves open, with why, and asks nothing where it closes
//! none. A confirmation holds what it named until it is answered: news that move rows or
//! bring runs leave its runs as they were, and confirming it closes exactly those, what
//! refused or failed then noted. No act asks for the field: its runs move as their news
//! come. `d` lists the closed too and hides them again; `e` reaches the whole machine and
//! back, in a repository: outside one the field stands at the machine's reach alone, and `e`
//! does nothing and nothing names it; `esc` does nothing, and `q` leaves. Help lists every key the field takes, those its
//! legend leaves out among them, in render's words. Opening the run pointed at, where the
//! field reads it, or the one the attention row names, with `a`, opens its stage over the
//! field; so do the workflows, with `w`, doctor, with `m`, and a new run, with `n`, which reads
//! what it needs as it opens, where a run can be asked for (in a repository set up: outside
//! one the field at the machine's reach neither draws its first row nor names `n`); what the
//! run pointed at cost, with `s`, opens over its screen, as a person walks there. A person who comes back to the
//! field finds it as the state keeps it, and nothing is asked; one who leaves a run for it
//! finds noted there, until the next key, whether the run keeps going, waits or ended.

use chrono::{DateTime, Utc};
use surco::components::{Legend, Note, Noted, Region, Screen};
use surco::doc::{Depth, Doc};
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_api::{FieldReach, FieldReading};
use yunta_core::step::{Arg, NextStep};
use yunta_core::RunId;
use yunta_render::page::{FieldRows, Page};

use super::{Stage, Want};
use crate::followed_runs::Standpoint;
use crate::prompt::Stroke;
use crate::{FollowedRuns, Outcome};

mod acts;
mod glance;

use acts::{attended, CLOSES, RESUMES, STOPS};

/// The field a person moves through: where they stand in it, and how far and how deep
/// it reads. Not the field's runs, which the state keeps of what it follows.
#[derive(Debug, Clone)]
pub struct FieldStage {
    /// The run a key acts on, where a person moved onto one: where the field does not
    /// list it, its first run stands for it.
    pointed: Option<RunId>,
    /// The runs picked for an act that takes several, in the order they were picked.
    marked: Vec<RunId>,
    pub(super) reach: FieldReach,
    /// How deep it lists the runs: at a glance, or the closed too.
    pub(super) depth: Depth,
    /// What the last act came to where it did not do what it was asked, or what leaving a
    /// run said of it: noted until the next key.
    said: Option<FieldNote>,
}

/// What the field notes in its attention row's place until the next key, under the mark of
/// how it went. Not Surco's `Noted`, the note as a screen holds it.
#[derive(Debug, Clone)]
struct FieldNote {
    mark: Mark,
    said: String,
}

impl FieldStage {
    /// The field as far as `reach`, at a glance, the pointer on its first run.
    pub(crate) fn reaching(reach: FieldReach) -> Self {
        FieldStage {
            pointed: None,
            marked: Vec::new(),
            reach,
            depth: Depth::Glance,
            said: None,
        }
    }

    /// `said` noted on the field until the next key, where anything was said: what leaving a
    /// run for it said of the run.
    pub(crate) fn noting(&mut self, said: Option<String>) {
        self.said = said.map(|said| FieldNote {
            mark: Mark::Done,
            said,
        });
    }

    /// The field `followed` keeps, framed at `now`: what the stage draws and acts on; none
    /// until it is read. A field read at the reach before stands until the field at this one
    /// comes in its place.
    fn framed(followed: &FollowedRuns, now: DateTime<Utc>) -> Option<FieldReading> {
        Some(followed.field.as_ref()?.reading(now))
    }

    /// `framed` as render lists it for a screen, the pointer on `pointed`, its first row asking
    /// for a run where one `asks_for_a_run` here.
    fn rows<'a>(
        &'a self,
        framed: &'a FieldReading,
        pointed: Option<&'a RunId>,
        asks_for_a_run: bool,
    ) -> FieldRows<'a> {
        FieldRows {
            rows: &framed.rows,
            unreadable: &framed.unreadable,
            depth: self.depth,
            pointed,
            marked: &self.marked,
            asks_for_a_run,
        }
    }

    /// The runs the list holds of `framed`, in its order, which its first row is none of: none
    /// where the field is not read.
    fn listed(&self, framed: Option<&FieldReading>) -> Vec<RunId> {
        framed
            .map(|framed| {
                let rows = self.rows(framed, None, false);
                rows.listed().into_iter().cloned().collect()
            })
            .unwrap_or_default()
    }

    /// The run a key acts on among `listed`: the one pointed at where it is listed, the
    /// first run listed otherwise.
    fn pointed_in(&self, listed: &[RunId]) -> Option<RunId> {
        let pointed = self.pointed.as_ref().filter(|run| listed.contains(run));
        pointed.or(listed.first()).cloned()
    }

    /// The pointer and the picks kept to `listed`: the picks whose runs it lists, and the
    /// pointer on its run where it lists it, on the first run otherwise.
    fn kept_to(&mut self, listed: &[RunId]) {
        self.marked.retain(|run| listed.contains(run));
        self.pointed = self.pointed_in(listed);
    }

    /// The field `followed` keeps as a screen at `now`, a line typed over it where `typed`:
    /// its list; the run pointed at, at a glance beside it, with what a person can do about it;
    /// where it stands on the trail; the tally and the attention row; the note; and the keys.
    /// A look that failed is said under the list.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        let framed = FieldStage::framed(followed, now);
        let pointed = framed
            .as_ref()
            .and_then(|framed| self.pointed_in(&self.listed(Some(framed))));
        let asks_for_a_run = followed.can_ask_for_a_run();
        let mut doc = match &framed {
            Some(framed) => self
                .rows(framed, pointed.as_ref(), asks_for_a_run)
                .document(look),
            None => Doc::new().with(Note(yunta_render::screen::READING_THE_RUNS.to_string())),
        };
        if let Some(failed) = &followed.look_failed {
            doc = doc.with(Note(failed.clone()));
        }
        let mut screen = Screen::new(Region::new(doc).listing_runs());
        screen.side = framed
            .as_ref()
            .zip(pointed.as_ref())
            .and_then(|(framed, pointed)| glance::beside(followed, framed, pointed, look));
        let place = framed.as_ref().and_then(|framed| place(followed, framed));
        screen.trail = std::iter::once(yunta_render::screen::YUNTA.to_string())
            .chain(place)
            .collect();
        let at = Standpoint { run: None, typed };
        (screen.tally, screen.attention) = followed.framing(at, now);
        screen.noted = self.said.as_ref().map(|note| Noted {
            mark: note.mark,
            said: note.said.clone(),
        });
        let acts_on_a_run = self.acts_on_a_run(framed.as_ref(), followed);
        screen.legend = yunta_render::screen::field_legend(asks_for_a_run, acts_on_a_run);
        screen.anywhere = match typed {
            true => Legend::new(),
            false => yunta_render::screen::anywhere(),
        };
        screen
    }

    /// Whether the keys that act on a run serve on `framed`, the field `followed` keeps, as
    /// its legend names them: where it lists a run in sight, and where it may yet list one,
    /// before it is read and while a run started here is on its way into it. A field read
    /// that lists none and awaits none has nothing to move to, open or close: its runs closed
    /// and hidden, or none at all.
    fn acts_on_a_run(&self, framed: Option<&FieldReading>, followed: &FollowedRuns) -> bool {
        match framed {
            None => true,
            Some(framed) => !self.listed(Some(framed)).is_empty() || followed.a_run_is_coming(),
        }
    }

    /// Every key the field takes, as help lists them over it: those its legend leaves out
    /// among them, `d` and `e` saying what they do from as deep and as far as it reads, and a
    /// new run where `followed` says one can be asked for here.
    pub(crate) fn help(&self, followed: &FollowedRuns) -> Doc {
        let closed = self.depth.reaches(Depth::Full);
        let asks_for_a_run = followed.can_ask_for_a_run();
        let farther = self.farther(followed);
        yunta_render::screen::help_over_the_field(closed, asks_for_a_run, farther.as_ref())
    }

    /// What `stroke` asks for in the field `followed` keeps at `now`, the pointer and the
    /// picks first kept to what it lists.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        self.said = None;
        let framed = FieldStage::framed(followed, now);
        let listed = self.listed(framed.as_ref());
        if framed.is_some() {
            self.kept_to(&listed);
        }
        match stroke {
            Stroke::Down | Stroke::Insert('j') => self.moved(1, &listed),
            Stroke::Up | Stroke::Insert('k') => self.moved(-1, &listed),
            Stroke::Enter => opened(self.read_pointed(&listed, followed)),
            Stroke::Insert('n') if followed.can_ask_for_a_run() => Want::taking(&NextStep::Run {
                workflow: Arg::Blank,
            }),
            Stroke::Insert(' ') => self.picked(&listed),
            RESUMES => self.resuming(&listed, followed),
            STOPS => self.stopping(&listed, followed),
            CLOSES => self.closing(&listed, followed, framed.as_ref()),
            Stroke::Insert('s') => self
                .read_pointed(&listed, followed)
                .map(what_it_cost)
                .unwrap_or_default(),
            Stroke::Insert('a') => opened(attended(followed, now)),
            Stroke::Insert('d') => {
                self.depth = match self.depth.reaches(Depth::Full) {
                    true => Depth::Glance,
                    false => Depth::Full,
                };
                Vec::new()
            }
            Stroke::Insert('e') => {
                self.reach = self.reach_by_e(followed).unwrap_or(self.reach);
                Vec::new()
            }
            Stroke::Insert('w') => Want::taking(&NextStep::List),
            Stroke::Insert('m') => Want::taking(&NextStep::Doctor {
                sessions: false,
                full: false,
            }),
            Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
            _ => Vec::new(),
        }
    }

    /// What an answer to work the field asked for does: what [`FieldStage::noted`] notes.
    /// The field moves as its runs do, so no answer asks for it.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        self.noted(&outcome);
        Vec::new()
    }

    /// What an act on runs came to, noted, whether the field asked it or a stage left asked
    /// it: the runs closed no longer picked, and what refused or failed said; a run handed back
    /// to its resume noted.
    pub(crate) fn noted(&mut self, outcome: &Outcome) {
        let (mark, said) = match outcome {
            Outcome::Resumed { run } => (Mark::Done, yunta_render::start::resumed_note(run)),
            Outcome::Closed {
                closed,
                refused,
                failed,
            } => {
                self.marked.retain(|run| !closed.contains(run));
                match (failed, yunta_render::screen::not_closed(refused)) {
                    (Some(failed), _) => (Mark::Failed, failed.clone()),
                    (None, Some(refused)) => (Mark::Done, refused),
                    (None, None) => {
                        self.said = None;
                        return;
                    }
                }
            }
            Outcome::Refused(refused) => (Mark::Done, refused.to_string()),
            Outcome::Failed(said) => (Mark::Failed, said.clone()),
            Outcome::Cancelled { .. }
            | Outcome::Catalog(_)
            | Outcome::Repository(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::Started { .. }
            | Outcome::PullRequest(_)
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Doctor(_)
            | Outcome::Collected(_)
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Detected { .. }
            | Outcome::SetupPack { .. }
            | Outcome::Initialized { .. }
            | Outcome::Stopped => return,
        };
        self.said = Some(FieldNote { mark, said });
    }

    /// The pointer moved `by` rows along `listed`, around at either end.
    fn moved(&mut self, by: isize, listed: &[RunId]) -> Vec<Want> {
        if listed.is_empty() {
            return Vec::new();
        }
        let at = self
            .pointed_in(listed)
            .and_then(|run| listed.iter().position(|listed| *listed == run))
            .unwrap_or(0);
        let to = (at as isize + by).rem_euclid(listed.len() as isize) as usize;
        self.pointed = Some(listed[to].clone());
        Vec::new()
    }

    /// The run pointed at among `listed` picked, or put back.
    fn picked(&mut self, listed: &[RunId]) -> Vec<Want> {
        if let Some(run) = self.pointed_in(listed) {
            match self.marked.iter().position(|marked| *marked == run) {
                Some(at) => {
                    self.marked.remove(at);
                }
                None => self.marked.push(run),
            }
        }
        Vec::new()
    }
}

/// Where the field `reading` stands on the trail: the repository's path the repository's
/// reading in `followed` gave, where it lists that repository's runs; this machine, where it
/// lists every run on it. None before the repository is read.
fn place(followed: &FollowedRuns, reading: &FieldReading) -> Option<String> {
    let home = followed.home.as_deref();
    match reading.here {
        true => {
            let repository = followed.repository.as_ref()?.dir()?;
            Some(yunta_render::screen::field_place(Some(repository), home))
        }
        false => Some(yunta_render::screen::field_place(None, home)),
    }
}

/// What `run` cost, opened as a person walks there from the field: its screen over the
/// field, then its numbers over its screen, which going back walks the same way.
fn what_it_cost(run: RunId) -> Vec<Want> {
    let screen = Stage::opened_by(&NextStep::status(&run));
    let numbers = Stage::opened_by(&NextStep::Stats { run });
    screen.into_iter().chain(numbers).map(Want::Open).collect()
}

/// `run` followed in its stage, opened over the field, where there is a run.
fn opened(run: Option<RunId>) -> Vec<Want> {
    run.map(|run| Want::taking(&NextStep::status(&run)))
        .unwrap_or_default()
}
