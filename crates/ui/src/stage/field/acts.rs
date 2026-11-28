//! See [`super`]. What the field's keys do to the runs they name, and what the field offers
//! the palette. A key acts on a run only where the acts the follower keeps for it hold the act
//! the key takes, by the one table of keys the glance names them by (`glance::key_of`), so it
//! acts on no run the field cannot read: `r` resumes at once, `c` stops once confirmed, and `x`
//! closes the runs picked, or the one pointed at, after one confirmation that names each run
//! it closes and each it leaves open, with why. Why a close leaves a run open is what a close
//! of it refuses, by the engine's one rule as the follower reads it
//! (`FollowedRun::close_refused`), else what holds it as its row says it: worked out as the key
//! is pressed, never at a draw, and held by the confirmation until it is answered.

use chrono::{DateTime, Utc};
use surco::glyphs::Mark;
use yunta_api::{FieldReach, FieldReading, FollowedRun};
use yunta_core::step::{Arg, NextStep, Reach};
use yunta_core::RunId;
use yunta_render::page::FieldRow;
use yunta_render::screen::{Confirming, LeftOpen, WhyLeftOpen};

use super::{glance, FieldNote, FieldStage};
use crate::dialog::Confirmation;
use crate::followed_runs::{Attended, Standpoint};
use crate::prompt::Stroke;
use crate::stage::{Act, Want};
use crate::{FollowedRuns, Operation};

/// The key that resumes the run pointed at.
pub(super) const RESUMES: Stroke = Stroke::Insert('r');

/// The key that stops the run pointed at, once confirmed.
pub(super) const STOPS: Stroke = Stroke::Insert('c');

/// The key that closes the runs picked, or the one pointed at, once confirmed.
pub(super) const CLOSES: Stroke = Stroke::Insert('x');

impl FieldStage {
    /// What the field `followed` keeps at `now` offers, each with the key that takes it
    /// here, in the order the palette lists them: following the run pointed at, where the
    /// field reads it, and each act its kept acts hold that a key here takes; closing what `x`
    /// closes where one command does it; what the run pointed at cost; the run the attention
    /// row names; a new run, by `n`, where one can be asked for; then the field's lists
    /// ([`FieldStage::lists`]).
    pub(crate) fn offered(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Act> {
        let framed = FieldStage::framed(followed, now);
        let listed = self.listed(framed.as_ref());
        let read = self.read_pointed(&listed, followed);
        let following = read
            .iter()
            .map(|run| Act::by(Stroke::Enter, NextStep::Follow { run: run.clone() }));
        let acts = read
            .iter()
            .filter_map(|run| kept(followed, run))
            .flat_map(|kept| kept.actions())
            .filter_map(|action| {
                let key = glance::key_of(&action.step).filter(|key| *key != CLOSES)?;
                Some(Act::by(key, action.step.clone()))
            });
        let closing = self
            .closed_by_one_command(&listed, followed)
            .map(|run| Act::by(CLOSES, NextStep::Close { run }));
        let cost = read
            .iter()
            .map(|run| Act::by(Stroke::Insert('s'), NextStep::Stats { run: run.clone() }));
        let needs = attended(followed, now).map(|_| Act::by(Stroke::Insert('a'), NextStep::Needs));
        let new_run = followed.can_ask_for_a_run().then(|| {
            Act::by(
                Stroke::Insert('n'),
                NextStep::Run {
                    workflow: Arg::Blank,
                },
            )
        });
        following
            .chain(acts)
            .chain(closing)
            .chain(cost)
            .chain(needs)
            .chain(new_run)
            .chain(self.lists(followed))
            .collect()
    }

    /// The lists the field offers, where the application stands as `followed` keeps it: this
    /// repository's runs where no key here lists them, in a repository; the field with its
    /// closed runs, or without, by `d`, as far as it reaches; as far again, by `e`, as deep
    /// as it lists, where it reaches anywhere else ([`FieldStage::farther`]); each by the
    /// command that lists the same ([`listing`]); the workflows and doctor.
    fn lists(&self, followed: &FollowedRuns) -> Vec<Act> {
        let full = self.depth.reaches(surco::doc::Depth::Full);
        let closed = Some(listing(self.reach, !full));
        let farther = self.farther(followed);
        let here = NextStep::ListRuns {
            reach: Reach::Repository,
        };
        let by_a_key = [&closed, &farther];
        let listed_by_no_key = (followed.in_a_repository()
            && !by_a_key.contains(&&Some(here.clone())))
        .then(|| Act::unbound(here));
        let doctor = NextStep::Doctor {
            sessions: false,
            full: false,
        };
        let keyed = [
            (Stroke::Insert('d'), closed),
            (Stroke::Insert('e'), farther),
        ];
        listed_by_no_key
            .into_iter()
            .chain(
                keyed
                    .into_iter()
                    .filter_map(|(key, step)| Some(Act::by(key, step?))),
            )
            .chain([
                Act::by(Stroke::Insert('w'), NextStep::List),
                Act::by(Stroke::Insert('m'), doctor),
            ])
            .collect()
    }

    /// The run pointed at among `listed`, where what `followed` keeps holds it: one the field
    /// reads, which opening and what it cost take.
    pub(super) fn read_pointed(&self, listed: &[RunId], followed: &FollowedRuns) -> Option<RunId> {
        let pointed = self.pointed_in(listed);
        pointed.filter(|run| kept(followed, run).is_some())
    }

    /// Resuming the run pointed at among `listed`, at once, where what `followed` keeps of it
    /// offers its resume; where it waits on a decision, which no resume answers, the note that
    /// says so and that `enter` reads it; nothing elsewhere.
    pub(super) fn resuming(&mut self, listed: &[RunId], followed: &FollowedRuns) -> Vec<Want> {
        let Some(run) = self.pointed_in(listed) else {
            return Vec::new();
        };
        if offers(followed, &run, RESUMES) {
            return vec![Want::Work(Operation::Resume { run })];
        }
        if kept(followed, &run).is_some_and(|kept| kept.decision().is_some()) {
            self.said = Some(FieldNote {
                mark: Mark::Done,
                said: yunta_render::screen::NOTHING_TO_RESUME.to_string(),
            });
        }
        Vec::new()
    }

    /// Stopping the run pointed at among `listed`, asked first, where what `followed` keeps of
    /// it offers its stop.
    pub(super) fn stopping(&self, listed: &[RunId], followed: &FollowedRuns) -> Vec<Want> {
        let pointed = self.pointed_in(listed);
        let Some(run) = pointed.filter(|run| offers(followed, run, STOPS)) else {
            return Vec::new();
        };
        let asks = Confirming::Stop {
            run: run.handle().to_string(),
        };
        let same = vec![NextStep::Cancel { run: run.clone() }];
        vec![Want::Confirm(Confirmation::new(
            asks,
            same,
            Operation::Cancel { run },
        ))]
    }

    /// The runs picked among `listed`, in the order they were picked.
    fn marked_in(&self, listed: &[RunId]) -> Vec<RunId> {
        let marked = self.marked.iter().filter(|run| listed.contains(run));
        marked.cloned().collect()
    }

    /// The runs `x` names among `listed`, split by whether what `followed` keeps of each offers
    /// its close: the runs picked, in the order they were picked, or the one pointed at where
    /// none is; those it closes first, then those it leaves open.
    fn named_by_x(&self, listed: &[RunId], followed: &FollowedRuns) -> (Vec<RunId>, Vec<RunId>) {
        let named = match self.marked_in(listed) {
            marked if marked.is_empty() => self.pointed_in(listed).into_iter().collect(),
            marked => marked,
        };
        named
            .into_iter()
            .partition(|run| offers(followed, run, CLOSES))
    }

    /// The one run `x` closes among `listed`, where it closes one. Several closed together
    /// are closed by no one command, so the palette offers that by its key alone.
    fn closed_by_one_command(&self, listed: &[RunId], followed: &FollowedRuns) -> Option<RunId> {
        match self.named_by_x(listed, followed).0.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        }
    }

    /// Closing the runs `x` names among `listed` that offer their close, asked first with
    /// each named: the runs it closes in its question, and under its sentence each run it
    /// leaves open, with why, as `framed` reads it. Nothing is asked where none is closed.
    pub(super) fn closing(
        &self,
        listed: &[RunId],
        followed: &FollowedRuns,
        framed: Option<&FieldReading>,
    ) -> Vec<Want> {
        let (runs, open) = self.named_by_x(listed, followed);
        if runs.is_empty() {
            return Vec::new();
        }
        let left = open
            .iter()
            .map(|run| LeftOpen {
                run: run.handle().to_string(),
                why: why_left_open(followed, framed, run),
            })
            .collect();
        let asks = Confirming::Close {
            runs: runs.iter().map(|run| run.handle().to_string()).collect(),
            left,
        };
        let same = runs
            .iter()
            .map(|run| NextStep::Close { run: run.clone() })
            .collect();
        vec![Want::Confirm(Confirmation::new(
            asks,
            same,
            Operation::Close { runs },
        ))]
    }
}

/// `run` as what `followed` keeps of the field holds it, where the field reads it.
fn kept<'a>(followed: &'a FollowedRuns, run: &RunId) -> Option<&'a FollowedRun> {
    let field = followed.field.as_ref()?;
    field.runs.get(run).map(|kept| &**kept)
}

/// The run the attention row of the field names at `now`, by the rule every screen that
/// draws the row follows, where it names one: where `a` goes.
pub(super) fn attended(followed: &FollowedRuns, now: DateTime<Utc>) -> Option<RunId> {
    let at = Standpoint {
        run: None,
        typed: false,
    };
    match followed.attended(at, now)? {
        Attended::Another(named) => Some(named),
        Attended::ItsOwn => None,
    }
}

/// The step that lists the field as far as `reach` and with its closed runs where `closed`,
/// by the command that lists the same: this repository's runs (`--full` with the closed),
/// every run on this machine (`--all`, and `--full` with the closed).
fn listing(reach: FieldReach, closed: bool) -> NextStep {
    let reach = match reach {
        FieldReach::Here => Reach::Repository,
        FieldReach::Machine => Reach::Machine,
    };
    let reach = match closed {
        true => reach.with_closed(),
        false => reach,
    };
    NextStep::ListRuns { reach }
}

impl FieldStage {
    /// How far `e` takes the field from where it reaches, where `followed` keeps the
    /// application in a repository: every run on this machine from this repository's, and
    /// back. None outside a repository, where the field stands at the machine's reach alone.
    pub(super) fn reach_by_e(&self, followed: &FollowedRuns) -> Option<FieldReach> {
        match self.reach {
            FieldReach::Here => Some(FieldReach::Machine),
            FieldReach::Machine => followed.in_a_repository().then_some(FieldReach::Here),
        }
    }

    /// The step `e` takes from the field, as deep as it lists ([`listing`]), where it takes
    /// one ([`FieldStage::reach_by_e`]).
    pub(super) fn farther(&self, followed: &FollowedRuns) -> Option<NextStep> {
        let full = self.depth.reaches(surco::doc::Depth::Full);
        self.reach_by_e(followed).map(|reach| listing(reach, full))
    }
}

/// Whether what `followed` keeps of `run` offers the act the field's `key` takes: a key acts
/// on a run only where its kept acts hold that act, so on none the field cannot read.
fn offers(followed: &FollowedRuns, run: &RunId, key: Stroke) -> bool {
    kept(followed, run).is_some_and(|kept| {
        let mut steps = kept.actions().iter().map(|action| &action.step);
        steps.any(|step| glance::key_of(step) == Some(key))
    })
}

/// Why a close leaves `run` open where its kept acts offer no close: what a close of it
/// refuses, by the engine's rule; else what holds it, as its row in `framed` says it, or why
/// the field cannot read it.
fn why_left_open(
    followed: &FollowedRuns,
    framed: Option<&FieldReading>,
    run: &RunId,
) -> WhyLeftOpen {
    if let Some(refused) = kept(followed, run).and_then(FollowedRun::close_refused) {
        return WhyLeftOpen::Refused(refused);
    }
    let row = framed
        .into_iter()
        .flat_map(|framed| &framed.rows)
        .find(|row| row.frame.run_id == *run)
        .map(FieldRow::said);
    let unread = framed
        .into_iter()
        .flat_map(|framed| &framed.unreadable)
        .find(|unread| unread.run_id == *run)
        .map(|unread| unread.problem.to_string());
    WhyLeftOpen::Holding(row.or(unread).unwrap_or_default())
}
