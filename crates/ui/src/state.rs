//! See [`crate`]. The application as one value: where the person is, what stands over
//! it, the work it waits on, and the instant it stands at. An input changes it and
//! says what it asks for; the screen is read off it.
//!
//! The state hands out a [`Ticket`] for each piece of work it asks for and keeps the
//! ones it waits on. An answer under a ticket it no longer waits on is dropped, so
//! leaving a stage is never undone by a late answer. Work already under way is not
//! asked twice, and a reading asked again lets go the one of its kind under way.
//!
//! The application first stands where the repository's reading says outside a
//! repository and in one never set up. In one set up it follows the field here and stands
//! once the field's first news is folded: on the field where it holds a run, read or
//! not, on the first request where it holds none. Until then its screen says the runs are
//! being read, and no reading of the field is asked: what is followed is the one reading.
//!
//! Opened on a step, the application first stands where it would open, then opens over
//! it the stages a person walks through to the step, so going back walks the same way.
//!
//! A stage opens over another, which the person comes back to; leaving a stage either
//! way lets go the readings it waits on, and what the performer holds for it, while an
//! act goes on and what it comes to is said where the person is. A stroke goes to the dialog standing over the stage when
//! there is one, and to the stage otherwise; news of work a stage asked for goes to it.
//!
//! Leaving a run ends on the field: the state goes down to the stage at the bottom of what
//! stands under the run, which the field takes the place of where it is another, as the
//! first request is once a run was started from it, and the field notes what leaving said.
//!
//! What the application follows is a function of where the person is, `followed`'s
//! ([`State::followed`]): each input ends by asking for it where it changed, never twice
//! the same and under no ticket, since a following is never let go. What is followed
//! folds into what the state keeps of it, for every stage to read; a promotion's successor
//! takes the place of its run under every stage that shows it, so it is followed from
//! then on; the stage the person is at then says what the piece folded asks of it
//! (`Stage::followed_on`). A run kept that turns to need the person once the field was
//! read whole calls them, and the screen's title names one that needs them (`calling`).
//!
//! The state is `Clone`: a test branches it once per input.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use yunta_api::{FieldReach, Repository};
use yunta_core::step::NextStep;

use crate::dialog::confirmation::Confirmed;
use crate::dialog::help::{self, Helped};
use crate::dialog::palette::{self, Chosen};
use crate::dialog::{Dialog, Help, Palette};
use crate::prompt::Stroke;
use crate::stage::{FieldStage, SetupStage, Want};
use crate::{FollowedRuns, Input, Operation, Outcome, RepositoryReading, Request, Stage, Ticket};

mod attending;
mod calling;
mod followed;
mod screen;

/// The application as one value.
#[derive(Debug, Clone)]
pub struct State {
    /// Where the person is; nothing until the application knows where it stands.
    stage: Option<Stage>,
    /// The stages the one the person is at was opened over, the nearest last.
    under: Vec<Stage>,
    /// What stands over the stage until it is answered.
    dialog: Option<Dialog>,
    /// What stopped the application learning where it stands, where something did.
    unopened: Option<String>,
    /// The stages a step opened the application on, opened in order over where it
    /// first stands, as a person walks there; none once they are.
    opened_on: Vec<Stage>,
    /// The repository the application opened in, as far as Yunta went there, once it
    /// knows: what it opens on.
    repository: Option<Repository>,
    /// The work the state asked for and waits on, by its ticket.
    waiting: BTreeMap<Ticket, Operation>,
    /// The ticket the next piece of work goes under.
    next: Ticket,
    /// The instant the application stands at.
    now: DateTime<Utc>,
    /// What it follows, as it last asked, and what it keeps of it.
    followed_runs: FollowedRuns,
    /// Which runs it keeps need the person, so a run that turns to need them calls them.
    calling: calling::Calling,
}

impl State {
    /// How often a surface hands in the time: a screen says ages and how long a run has
    /// gone to the second, so a second is how far its words can stand behind the clock.
    pub const TIME_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

    /// The application as it opens at `now`, and what it asks first: the repository it
    /// works in, and whether Yunta was set up there. It stands on nothing until that is
    /// answered.
    pub fn opened(now: DateTime<Utc>) -> (State, Vec<Request>) {
        let mut state = State {
            stage: None,
            under: Vec::new(),
            dialog: None,
            unopened: None,
            opened_on: Vec::new(),
            repository: None,
            waiting: BTreeMap::new(),
            next: Ticket::first(),
            now,
            followed_runs: FollowedRuns::new(),
            calling: calling::Calling::default(),
        };
        let asked = state.asked(Operation::ReadRepository);
        (state, asked)
    }

    /// The application as it opens at `now` on `step`, and what it asks first: it reads
    /// where it stands as [`State::opened`] does and stands there, then opens over it the
    /// run's stage, for a step of a run that shows something over the run, the stage
    /// `status` opens, and over that the stage `step` opens, as a person walks there; going
    /// back walks the same way. A step that shows the run itself, `review` and `resolve-gate`
    /// among them, opens the run's stage alone. None for a step no stage takes.
    pub fn opened_on(now: DateTime<Utc>, step: &NextStep) -> Option<(State, Vec<Request>)> {
        let opens = Stage::opened_by(step)?;
        let (mut state, asked) = State::opened(now);
        let run = step
            .run()
            .filter(|_| !matches!(opens, Stage::Run(_)))
            .and_then(|run| Stage::opened_by(&NextStep::status(run)));
        state.opened_on = run.into_iter().chain([opens]).collect();
        Some((state, asked))
    }

    /// What `input` does: the state it leaves, and what it asks for, what it follows last.
    pub fn update(&mut self, input: Input) -> Vec<Request> {
        let mut asked = self.taken(input);
        asked.extend(self.followed_runs.asked(self.followed()));
        asked
    }

    /// What `input` does but for what the state asks to follow.
    fn taken(&mut self, input: Input) -> Vec<Request> {
        match input {
            Input::Stroke(stroke) => self.pressed(stroke),
            Input::Laid(laid) => {
                if let Some(Dialog::Help(help)) = self.dialog.as_mut() {
                    help.laid(laid.overlay.unwrap_or_default());
                }
                if let Some(stage) = self.stage.as_mut() {
                    stage.laid(laid, &self.followed_runs, self.now);
                }
                Vec::new()
            }
            Input::Remark(ticket, remark) => {
                if let (true, Some(stage)) = (self.waiting.contains_key(&ticket), &mut self.stage) {
                    stage.remarked(remark);
                }
                Vec::new()
            }
            Input::News(ticket, news) => {
                match (self.waiting.contains_key(&ticket), &mut self.stage) {
                    (true, Some(stage)) => {
                        let wants = stage.heard(news);
                        self.wanted(wants)
                    }
                    (false, _) | (true, None) => Vec::new(),
                }
            }
            Input::Time(now) => {
                self.now = now;
                Vec::new()
            }
            Input::Outcome(ticket, outcome) => match self.waiting.remove(&ticket) {
                Some(asked) => self.answered(&asked, outcome),
                None => Vec::new(),
            },
            Input::FollowNews(news) => self.folded(news),
            Input::LookFailed(why) => {
                self.followed_runs.look_failed = Some(why);
                Vec::new()
            }
            Input::Unshown(address) => {
                if let Some(stage) = self.stage.as_mut() {
                    stage.unshown(&address);
                }
                Vec::new()
            }
        }
    }

    /// Whether a field is open where the person is, so a surface hands what is pasted
    /// in as typed text: anywhere else a paste answers nothing.
    pub fn typing(&self) -> bool {
        let typing = |stage: &Stage| stage.typing(&self.followed_runs, self.now);
        self.dialog.is_none() && self.stage.as_ref().is_some_and(typing)
    }

    /// The stages a step opened the application on, yet to open, outermost first: they
    /// stand nowhere on screen until the application first stands somewhere, so two
    /// states that show the same screen and wait on the same work can still differ by them.
    pub fn stages_to_open(&self) -> &[Stage] {
        &self.opened_on
    }

    /// The work the state waits on, each under its ticket, in the order it was asked.
    pub fn waiting_on(&self) -> Vec<(Ticket, Operation)> {
        self.waiting
            .iter()
            .map(|(ticket, operation)| (*ticket, operation.clone()))
            .collect()
    }

    /// What a stroke does: to the dialog standing over the stage, or to the stage; over a
    /// stage where nothing is typed, `/` opens the palette.
    fn pressed(&mut self, stroke: Stroke) -> Vec<Request> {
        if let Some(dialog) = self.dialog.take() {
            return self.at_the_dialog(dialog, stroke);
        }
        let (followed, now) = (&self.followed_runs, self.now);
        if let Some(stage) = self
            .stage
            .as_mut()
            .filter(|stage| !stage.typing(followed, now))
        {
            let opened = match stroke {
                palette::OPENS => Some(Dialog::Palette(Palette::over(
                    stage.offered(&self.followed_runs, self.now),
                    stage.showing(),
                ))),
                help::OPENS => Some(Dialog::Help(Help::default())),
                _ => None,
            };
            if opened.is_some() {
                stage.opened_over(&self.followed_runs);
                self.dialog = opened;
                return Vec::new();
            }
        }
        let wants = match (self.stage.as_mut(), &self.unopened) {
            (Some(stage), _) => stage.pressed(stroke, &self.followed_runs, self.now),
            (None, Some(_)) => match stroke {
                Stroke::Insert('q') | Stroke::Decline | Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
            (None, None) => match stroke {
                Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
        };
        self.wanted(wants)
    }

    /// What an answer to the work `asked` the state waited on does: the repository read,
    /// which says where the application opens, or what the stage makes of it; a run handed
    /// to a detached engine, started, resumed or answered, kept as handed on until a reading
    /// of that engine comes. A repository Yunta was never set up in opens on its setup.
    /// Where it first stands, the stages a step opened it on open over it.
    fn answered(&mut self, asked: &Operation, outcome: Outcome) -> Vec<Request> {
        if let Some(run) = outcome.handed_on() {
            self.followed_runs.handed_on(run.clone());
        }
        let wants = match (outcome, self.stage.as_mut()) {
            (Outcome::Repository(read), _) => self.read_in(*read),
            (Outcome::Refused(refused), None) => {
                self.unopened = Some(refused.to_string());
                Vec::new()
            }
            (Outcome::Failed(said), None) => {
                self.unopened = Some(said);
                Vec::new()
            }
            (outcome, Some(stage)) => stage.answered(asked, outcome),
            (_, None) => Vec::new(),
        };
        self.wanted(wants)
    }

    /// What the repository read as `read` does: where the project keeps its runs and its
    /// prices kept for every stage, then outside a repository the signpost, in one never
    /// set up its setup, and in one set up nowhere yet: the field here is followed, and its
    /// first news says where the application stands.
    fn read_in(&mut self, read: RepositoryReading) -> Vec<Want> {
        let RepositoryReading {
            repository,
            storage,
            prices,
        } = read;
        self.followed_runs.storage = Some(storage);
        self.followed_runs.prices = prices;
        match repository {
            Repository::Absent => self.stands_on(Stage::Signpost),
            Repository::NeverSetUp => self.stands_on(Stage::Setup(SetupStage::new())),
            Repository::SetUp => {
                self.repository = Some(repository);
                Vec::new()
            }
        }
    }

    /// The application standing on `stage`, where it first stands: what `stage` asks as
    /// it opens, then each stage a step opened the application on, opened over it.
    fn stands_on(&mut self, stage: Stage) -> Vec<Want> {
        let opening = stage.opening();
        self.stage = Some(stage);
        let walked = std::mem::take(&mut self.opened_on);
        opening
            .into_iter()
            .chain(walked.into_iter().map(Want::Open))
            .collect()
    }

    /// What `stroke` does at `dialog`, standing over the stage: answered, it closes and
    /// what was chosen is done, a confirmed act asked for by the stage that wanted it and
    /// a palette's row taken by its key or as its step is; otherwise it stays.
    fn at_the_dialog(&mut self, dialog: Dialog, stroke: Stroke) -> Vec<Request> {
        let wants = match dialog {
            Dialog::Confirmation(confirmation) => match confirmation.pressed(stroke) {
                Confirmed::Yes(operation) => match self.stage.as_mut() {
                    Some(stage) => stage.confirmed(operation),
                    None => vec![Want::Work(operation)],
                },
                Confirmed::No => Vec::new(),
                Confirmed::Leave => vec![Want::Leave],
                Confirmed::Stay => {
                    self.dialog = Some(Dialog::Confirmation(confirmation));
                    Vec::new()
                }
            },
            Dialog::Palette(mut palette) => match palette.pressed(stroke) {
                Chosen::Pressed(key) => match self.stage.as_mut() {
                    Some(stage) => stage.pressed(key, &self.followed_runs, self.now),
                    None => Vec::new(),
                },
                Chosen::Took(wants) => wants,
                Chosen::Closed => Vec::new(),
                Chosen::Leave => vec![Want::Leave],
                Chosen::Stay => {
                    self.dialog = Some(Dialog::Palette(palette));
                    Vec::new()
                }
            },
            Dialog::Help(mut help) => match help.pressed(stroke) {
                Helped::Closed => Vec::new(),
                Helped::Leave => vec![Want::Leave],
                Helped::Stay => {
                    self.dialog = Some(Dialog::Help(help));
                    Vec::new()
                }
            },
        };
        self.wanted(wants)
    }

    /// What a stage's `wants` ask for, carried out: work under a ticket of its own, work
    /// let go, a confirmation stood over the stage, a stage opened over it, going back,
    /// or leaving.
    fn wanted(&mut self, wants: Vec<Want>) -> Vec<Request> {
        let mut asked = Vec::new();
        for want in wants {
            match want {
                Want::Work(operation) => asked.extend(self.asked(operation)),
                Want::LetGo(operation) => asked.extend(self.let_go(|waited| *waited == operation)),
                Want::Release(ticket) => asked.push(Request::LetGo(ticket)),
                Want::Show(address) => asked.push(Request::ShowPage(address)),
                Want::Confirm(confirmation) => {
                    self.dialog = Some(Dialog::Confirmation(confirmation));
                }
                Want::Open(stage) => asked.extend(self.opened_over(stage)),
                Want::Replace(stage) => asked.extend(self.replaced_by(stage)),
                Want::Back => asked.extend(self.back()),
                Want::BackToItsDecision => asked.extend(self.back_to_its_decision()),
                Want::ToTheField(said) => asked.extend(self.down_to_the_field(said)),
                Want::Leave => asked.extend(self.leaving()),
            }
        }
        asked
    }

    /// `stage` opened over the one the person is at, which the readings it waits on are
    /// let go from, and what `stage` asks for as it opens.
    fn opened_over(&mut self, stage: Stage) -> Vec<Request> {
        let mut asked = self.let_go(Operation::reads);
        self.under.extend(self.stage.take());
        let opening = stage.opening();
        self.stage = Some(stage);
        asked.extend(self.wanted(opening));
        asked
    }

    /// `stage` in the place of the one the person is at, which the readings it waits on
    /// and what the performer holds for it are let go from, and what `stage` asks for as
    /// it opens; what that one was opened over stays under it.
    fn replaced_by(&mut self, stage: Stage) -> Vec<Request> {
        let mut asked = self.released();
        asked.extend(self.let_go(Operation::reads));
        let opening = stage.opening();
        self.stage = Some(stage);
        asked.extend(self.wanted(opening));
        asked
    }

    /// Back to the stage the one the person is at was opened over, the readings it
    /// waits on and what the performer holds for it let go, and what that stage asks for
    /// as the person comes back to it. Nothing where it was opened over none.
    fn back(&mut self) -> Vec<Request> {
        let Some(mut below) = self.under.pop() else {
            return Vec::new();
        };
        let mut asked = self.released();
        asked.extend(self.let_go(Operation::reads));
        let returned = below.returned(&self.followed_runs);
        self.stage = Some(below);
        asked.extend(self.wanted(returned));
        asked
    }

    /// Down to the field at the bottom of the stages, `said` noted on it: what the performer
    /// holds for each stage left, and the readings they wait on, let go. A field standing at
    /// the bottom stays as the person left it; any other stage there, the first request
    /// among them, gives way to the field the state keeps, at the reach it follows.
    fn down_to_the_field(&mut self, said: Option<String>) -> Vec<Request> {
        let mut asked = self.let_go(Operation::reads);
        let stages = std::mem::take(&mut self.under)
            .into_iter()
            .chain(self.stage.take());
        let mut bottom = None;
        for (at, stage) in stages.enumerate() {
            match (at, stage) {
                (0, Stage::Field(field)) => bottom = Some(field),
                (_, left) => asked.extend(left.held().map(Request::LetGo)),
            }
        }
        let reach = self.followed_runs.field.as_ref().map(|field| field.reach);
        let mut field =
            bottom.unwrap_or_else(|| FieldStage::reaching(reach.unwrap_or(FieldReach::Here)));
        field.noting(said);
        self.stage = Some(Stage::Field(field));
        asked
    }

    /// What the performer holds for the stage the person is at, let go: what leaving it
    /// drops.
    fn released(&self) -> Vec<Request> {
        self.stage
            .iter()
            .filter_map(Stage::held)
            .map(Request::LetGo)
            .collect()
    }

    /// Leaving the application: what the performer holds for the stage the person is at
    /// and for each under it let go, then leaving.
    fn leaving(&self) -> Vec<Request> {
        self.stage
            .iter()
            .chain(&self.under)
            .filter_map(Stage::held)
            .map(Request::LetGo)
            .chain([Request::Leave])
            .collect()
    }

    /// Every piece of work waited on that `which` picks, let go: no longer waited on,
    /// and stopped where it is under way.
    fn let_go(&mut self, which: impl Fn(&Operation) -> bool) -> Vec<Request> {
        let tickets: Vec<Ticket> = self
            .waiting
            .iter()
            .filter(|(_, waited)| which(waited))
            .map(|(ticket, _)| *ticket)
            .collect();
        tickets
            .into_iter()
            .map(|ticket| {
                self.waiting.remove(&ticket);
                Request::LetGo(ticket)
            })
            .collect()
    }

    /// Asks for `operation` under a ticket of its own, which the state then waits on:
    /// nothing where the same work is under way already, and, for a reading, the one of
    /// its kind under way let go first.
    fn asked(&mut self, operation: Operation) -> Vec<Request> {
        if self.waiting.values().any(|waited| *waited == operation) {
            return Vec::new();
        }
        let kind = operation.kind();
        // A reading supersedes the readings of its kind, never an act of it under way:
        // checking doctor again leaves its sessions running.
        let mut asked = match operation.reads() {
            true => self.let_go(|waited| waited.kind() == kind && waited.reads()),
            false => Vec::new(),
        };
        let ticket = self.next;
        self.next = ticket.next();
        self.waiting.insert(ticket, operation.clone());
        asked.push(Request::Perform(ticket, operation));
        asked
    }
}

#[cfg(test)]
mod tests;
