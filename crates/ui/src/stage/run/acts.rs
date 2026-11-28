//! See [`super`]. A run's keys, and the acts a key on its stage takes.
//!
//! The keys a run binds: `j k` along its nodes, the node pointed at shown in the pane as its
//! scene stands; the arrows (a wheel turn arrives as them), `PgUp`, `PgDn`, `Home`, `End` and
//! the space bar through the pane, the pointer left where it stands; `f` gives the lead back
//! to the run; `a` goes where the attention row says: to the run's own decision, the lead
//! given back to the run and its prompt in place as the person left it, or to the run it
//! names, whose screen takes this one's place;
//! `n` a new run where no prompt stands, before the run is read among it, and where a run can
//! be asked for, opened once the workflows are read; `q` and `esc` down to the field, which notes whether the run keeps going, an engine
//! driving it, waits until someone answers or resumes it, or ended, `esc` putting a prompt in
//! place aside first, where one stands, then going up to the run whose child the stage shows
//! in its place, where it shows one; and each act the run
//! offers that a key is bound to here. `enter` takes nothing beyond a prompt, which the
//! pointer needs no key to show; neither `p` nor `w` is bound, the plan being the gate's
//! own scene and the run's screen its status.
//!
//! The acts: `u` making room where the run stands still for want of it, upkeep opened over
//! the run; `r` going on from its log where an answer recorded awaits the resume that acts
//! on it and no engine takes it up; each step `RunFrame::actions` offers that a key here is
//! bound to, `r` going on, `x` closing, `c` stopping, `e` its receipt and `v` its
//! verification, `r` and `x` exactly where the legend names them (`Scene::named_acts`); `o`
//! the pull request the run waits on, shown by the surface by its link, where the scene
//! stands as it; what every run offers, `s` what it cost, `g` its graph, `o`
//! its page for a browser elsewhere, and its whole chronicle, which no key takes and the
//! palette says in words of its own, its command being following's; and a new run.
//! Stopping and closing ask a confirmation first; going on acts at once; upkeep collects
//! once its own confirmation is given; the receipt, the verification and the page open in
//! the evidence stage, what it cost and its graph in the stats and the graph stages, its
//! whole chronicle in its own.
//! Answering what the run waits on, `resolve-gate` with no option, is the palette's row of
//! `a` where `a` goes to the run's own decision, which it brings back in place.
//!
//! The legend names `u`, and `r` and `x` where the scene names them (`Scene::named_acts`),
//! `o` where it shows the pull request, in its words, and `s`, `g` and a new run in places
//! of its own; `c`, `e`, `v` and the run's page stand out of it, in the palette, which lists
//! each with its key; going on and closing where the legend does not name them with none,
//! and the run's page with none where `o` shows the pull request: a row with no key takes its
//! step as the palette takes any (`Want::taking`).
//! The pull request is no step a command line names, so the palette lists it not. Help lists
//! every act a key takes here, in the legend's words where the legend names it and in the
//! palette's elsewhere.

use chrono::{DateTime, Utc};
use yunta_core::events::GateWaitingPayload;
use yunta_core::step::{Arg, Choice, NextStep};
use yunta_core::RunId;
use yunta_render::command;
use yunta_render::screen::Leaving;
use yunta_replay::frame::{Effect, RunFrame, RunPhase};
use yunta_replay::registry::Engine;

use super::super::Want;
use super::scene::{ComposedPane, NamedActs, Scene};
use super::{pull_request, Framed, RunStage};
use crate::followed_runs::Attended;
use crate::{Address, FollowedRuns};

/// An act a run offers here: the key it is bound to, where one is, what it takes, what the
/// legend says it does, where the legend names it among the scene's own acts, and what the
/// palette says it does, where it says words of its own.
pub(super) struct Offered {
    pub(super) key: Option<char>,
    pub(super) takes: Takes,
    pub(super) named: Option<&'static str>,
    /// What its palette row says it does where another act shares its step's command, whose
    /// words are that act's: none where what its step does says it.
    pub(super) does: Option<&'static str>,
}

impl Offered {
    /// What it does, as help says it: in the legend's words where the legend names it, else
    /// in the palette's, its own or its step's.
    pub(super) fn said(&self) -> &'static str {
        match (self.named, self.does, &self.takes) {
            (Some(said), _, _) | (None, Some(said), _) => said,
            (None, None, Takes::Step(step)) => command::does(step),
            (None, None, Takes::Shows(_)) => yunta_render::screen::OPEN_THE_PULL_REQUEST,
        }
    }
}

/// What an act a run offers takes: a step, as a command line names it; or a page
/// elsewhere, which the surface shows. Not a [`NextStep`] alone, which names no page of a
/// forge's.
pub(super) enum Takes {
    Step(NextStep),
    Shows(Address),
}

/// What the run `framed` offers here, standing as `scene`: making room, where it stands
/// still for want of it; going on from its log, where an answer recorded awaits its resume
/// and no engine takes it up; each act its standing and `decision`, the decision it
/// waits on as read for the pause it stands at, offer that a key here takes, the legend
/// naming those the scene names; the pull request it waits on, shown by its link, where the
/// scene stands as it; then what every run offers, what it cost, its graph, its page for a
/// browser, bound to no key where `o` shows the pull request, and its whole chronicle; then
/// a new run, where no prompt stands in the scene and one `asks_for_a_run` here.
pub(super) fn offered(
    framed: &Framed<'_>,
    decision: Option<&GateWaitingPayload>,
    scene: Option<&Scene<'_>>,
    asks_for_a_run: bool,
) -> Vec<Offered> {
    let frame = framed.frame;
    let named_acts = scene.map_or(NamedActs::Neither, Scene::named_acts);
    let room = scene.filter(|scene| scene.makes_room()).map(|_| Offered {
        key: Some('u'),
        takes: Takes::Step(NextStep::Gc { dry_run: true }),
        named: Some(yunta_render::screen::MAKE_ROOM),
        does: None,
    });
    let actions = frame.actions(decision);
    let resumes = actions
        .iter()
        .any(|action| matches!(action.step, NextStep::Resume { .. }));
    let taken_up = (!resumes)
        .then(|| answers_resume(framed, named_acts))
        .flatten();
    let actions = actions.into_iter().filter_map(|action| {
        let key = bound(&action.step)?;
        let named = named_in_the_legend(&action.step, named_acts)
            .then(|| yunta_render::screen::in_the_legend(action.effect));
        Some(Offered {
            key: keyed(&action.step, named.is_some()).then_some(key),
            takes: Takes::Step(action.step),
            named,
            does: None,
        })
    });
    let shows = shows_the_pull_request(frame, scene);
    let others = every_run_offers(&frame.run_id, shows.is_none(), scene, asks_for_a_run);
    room.into_iter()
        .chain(taken_up)
        .chain(actions)
        .chain(shows)
        .chain(others)
        .collect()
}

/// The pull request the run framed as `frame` waits on, shown by `o` by its link, where
/// `scene` stands as it.
fn shows_the_pull_request(frame: &RunFrame, scene: Option<&Scene<'_>>) -> Option<Offered> {
    let pane = scene.map(Scene::composes);
    let link = (pane == Some(ComposedPane::PullRequest))
        .then(|| pull_request::link(frame))
        .flatten()?;
    Some(Offered {
        key: Some('o'),
        takes: Takes::Shows(Address::Link(link)),
        named: Some(yunta_render::screen::OPEN_THE_PULL_REQUEST),
        does: None,
    })
}

/// What every run offers, `run` here, standing as `scene`: what it cost; its graph; its page
/// for a browser, by `o` where `o_shows_the_page` and by no key elsewhere; its whole
/// chronicle, by no key, in the palette's words of its own; and a new run, where no prompt
/// stands in the scene and one `asks_for_a_run` here. The legend names none among the
/// scene's own.
fn every_run_offers(
    run: &RunId,
    o_shows_the_page: bool,
    scene: Option<&Scene<'_>>,
    asks_for_a_run: bool,
) -> impl Iterator<Item = Offered> {
    let always = [
        (Some('s'), NextStep::Stats { run: run.clone() }, None),
        (Some('g'), NextStep::RunGraph { run: run.clone() }, None),
        (
            o_shows_the_page.then_some('o'),
            NextStep::Open { run: run.clone() },
            None,
        ),
        (
            None,
            NextStep::Follow { run: run.clone() },
            Some(command::WHOLE_CHRONICLE),
        ),
    ];
    let prompt_stands = scene.is_some_and(|scene| scene.asks_in_place());
    let new_run = (asks_for_a_run && !prompt_stands).then(a_new_run);
    always
        .into_iter()
        .map(|(key, step, does)| Offered {
            key,
            takes: Takes::Step(step),
            named: None,
            does,
        })
        .chain(new_run)
}

/// A new run, by `n`: what a run offers where no prompt stands in its scene, and before the
/// run is read, where none stands yet, wherever a run can be asked for.
pub(super) fn a_new_run() -> Offered {
    Offered {
        key: Some('n'),
        takes: Takes::Step(NextStep::Run {
            workflow: Arg::Blank,
        }),
        named: None,
        does: None,
    }
}

/// Going on from the log of the run `framed`, where an answer recorded awaits the resume
/// that acts on it and no engine takes it up: by `r`, which the legend names, where the scene
/// names `named_acts` going on; listed with no key elsewhere.
fn answers_resume(framed: &Framed<'_>, named_acts: NamedActs) -> Option<Offered> {
    let named = names_its_resume(named_acts)
        .then(|| yunta_render::screen::in_the_legend(Effect::ContinuesFromLog));
    framed.answer_awaits_no_engine().then(|| Offered {
        key: named.is_some().then_some('r'),
        takes: Takes::Step(NextStep::Resume {
            run: framed.frame.run_id.clone(),
        }),
        named,
        does: None,
    })
}

/// Whether the key [`bound`] binds the act taking `step` to takes it here, where the legend
/// names it (`named`) or not: going on and closing exactly where the legend names them, the
/// palette listing them with no key elsewhere; stopping, the receipt and the verification
/// everywhere, the legend leaving them to the palette.
fn keyed(step: &NextStep, named: bool) -> bool {
    named || !matches!(step, NextStep::Resume { .. } | NextStep::Close { .. })
}

/// The key the act taking `step` is bound to here, where it takes it ([`keyed`]); none for
/// an act another stage takes. [`Want::taking`] says what each asks for.
fn bound(step: &NextStep) -> Option<char> {
    match step {
        NextStep::Cancel { .. } => Some('c'),
        NextStep::Resume { .. } => Some('r'),
        NextStep::Close { .. } => Some('x'),
        NextStep::Receipt { .. } => Some('e'),
        NextStep::Verify { .. } => Some('v'),
        _ => None,
    }
}

/// Whether the legend names the act taking `step` among the scene's own, where the scene
/// names `named_acts`: going on, and closing where it names both. Stopping, the receipt
/// and the verification stand in the palette.
fn named_in_the_legend(step: &NextStep, named_acts: NamedActs) -> bool {
    match named_acts {
        NamedActs::Neither | NamedActs::TheAnswersResume => false,
        NamedActs::GoingOn => matches!(step, NextStep::Resume { .. }),
        NamedActs::Both => matches!(step, NextStep::Resume { .. } | NextStep::Close { .. }),
    }
}

/// Whether the legend names the resume an answer recorded awaits, where the scene names
/// `named_acts`: wherever it names going on.
fn names_its_resume(named_acts: NamedActs) -> bool {
    match named_acts {
        NamedActs::Neither => false,
        NamedActs::TheAnswersResume | NamedActs::GoingOn | NamedActs::Both => true,
    }
}

/// What taking an act that takes `takes` asks for: a step, as [`Want::taking`] takes it; a
/// page elsewhere, shown by the surface.
pub(super) fn took(takes: Takes) -> Vec<Want> {
    match takes {
        Takes::Step(step) => Want::taking(&step),
        Takes::Shows(address) => vec![Want::Show(address)],
    }
}

/// What `q` and `esc` ask for on the run `framed`: down to the field, which notes where the
/// run is left; nothing is noted of a run not read yet.
pub(super) fn left(framed: Option<&Framed<'_>>) -> Vec<Want> {
    let said = framed.map(|framed| framed.leaving().said().to_string());
    vec![Want::ToTheField(said)]
}

impl Framed<'_> {
    /// Whether an answer a person recorded awaits the resume that acts on it and no engine
    /// is there to take it up: one the run is left waiting by ([`Framed::leaving`]).
    fn answer_awaits_no_engine(&self) -> bool {
        let awaits = self.in_view.run.state().answer_awaits_its_resume();
        awaits && self.leaving() == Leaving::Waits
    }

    /// Where the run is left when the person leaves its screen: ended once it closed; going
    /// on while its registry reads an engine alive, or, for a run this window handed to a
    /// detached engine itself, while that registry is not written yet, since the engine is
    /// about to write it; else waiting until someone answers or resumes it, a run no engine
    /// is read driving and this window did not hand on among them, whatever it stands at.
    /// The note `q` leaves and the legend's `q` read this one decision.
    pub(super) fn leaving(&self) -> Leaving {
        match &self.frame.phase {
            RunPhase::Finished
            | RunPhase::Failed { .. }
            | RunPhase::Cancelled
            | RunPhase::Promoted { .. }
            | RunPhase::Broken { .. } => return Leaving::Ended,
            RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. } => {}
        }
        match (self.in_view.run.engine(), self.handed_on) {
            (Engine::Alive { .. }, true | false) | (Engine::Unrecorded, true) => {
                Leaving::KeepsGoing
            }
            (Engine::Unrecorded, false) | (Engine::Dead | Engine::Unknown, true | false) => {
                Leaving::Waits
            }
        }
    }
}

impl RunStage {
    /// The step that answers what the run `framed` waits on here, where it stopped on a
    /// menu of its own or on a round of questions, read for the pause it stands at:
    /// `resolve-gate` with no option. None before what it asks is read, which stands by
    /// itself once read, and where it could not be read, which nothing here answers.
    pub(super) fn answering(&self, framed: Option<&Framed<'_>>) -> Option<NextStep> {
        let Framed { in_view, frame, .. } = framed?;
        let decision = in_view.awaited_decision();
        let answerable = decision.is_some() || in_view.awaited_round(frame).is_some();
        answerable.then(|| NextStep::ResolveGate {
            run: frame.run_id.clone(),
            choice: Choice::Asked,
        })
    }

    /// Where `a` takes the person from the run `framed`, as `followed` keeps it at `now`:
    /// where the attention row names, by the rule every screen that draws it follows
    /// (`FollowedRuns::attended`); where it names none, to the run's own decision where the
    /// run waits on one read and answered here, as where its prompt stands in sight; else
    /// nowhere, a decision not read yet among it, which stands by itself once read.
    pub(super) fn attended(
        &self,
        framed: Option<&Framed<'_>>,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Option<Attended> {
        let at = self.standpoint(framed, false);
        let answered_here = || self.answering(framed).map(|_| Attended::ItsOwn);
        followed.attended(at, now).or_else(answered_here)
    }

    /// The run's own decision, as `a` on a page over the run brings it from the run
    /// `followed` keeps at `now`, once the person is back on the run's stage: the lead given
    /// back to the run, its prompt standing in place as the person left it.
    pub(crate) fn attending_its_decision(
        &mut self,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        let framed = self.framed(followed, now);
        let wants = self.given_back(framed.as_ref());
        self.prompt_placed(framed.as_ref());
        wants
    }

    /// What `a` asks for on the run `framed`, as `followed` keeps it at `now`: on the run's
    /// own decision, the lead given back to the run, whose prompt stands in place again as
    /// the person left it; on another run, its screen in place of this one.
    pub(super) fn attending(
        &mut self,
        framed: Option<&Framed<'_>>,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        match self.attended(framed, followed, now) {
            Some(Attended::ItsOwn) => self.given_back(framed),
            Some(Attended::Another(run)) => Want::in_place(&run),
            None => Vec::new(),
        }
    }
}
