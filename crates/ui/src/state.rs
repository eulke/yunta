//! See [`crate`]. The application as one value: where the person is, what stands over
//! it, the work it waits on, and the instant it stands at. An input changes it and
//! says what it asks for; the screen is read off it.
//!
//! The state hands out a [`Ticket`] for each piece of work it asks for and keeps the
//! ones it waits on. An answer under a ticket it no longer waits on is dropped, so
//! leaving a stage is never undone by a late answer. Work already under way is not
//! asked twice, and a reading asked again lets go the one of its kind under way.
//!
//! Opened on a step, the application first stands where it would open, then opens over
//! it the stages a person walks through to the step, so going back walks the same way.
//!
//! A stage opens over another, which the person comes back to; leaving a stage either
//! way lets go the readings it waits on, and what the performer holds for it, while an
//! act goes on and what it comes to is said where the person is. A stroke goes to the dialog standing over the stage when
//! there is one, and to the stage otherwise; news of lasting work goes to the stage.
//! The state is `Clone`: a test branches it once per input.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use surco::components::{Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::{FieldReach, Repository};
use yunta_core::step::NextStep;

use crate::dialog::confirmation::Confirmed;
use crate::dialog::help::{self, Helped};
use crate::dialog::palette::{self, Chosen};
use crate::dialog::{Dialog, Help, Palette};
use crate::prompt::Stroke;
use crate::stage::{FieldStage, FirstRunStage, SetupStage, Want};
use crate::{Input, Operation, Outcome, Request, Stage, Ticket};

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
        };
        let asked = state.asked(Operation::ReadRepository);
        (state, asked)
    }

    /// The application as it opens at `now` on `step`, and what it asks first: it reads
    /// where it stands as [`State::opened`] does and stands there, then opens over it the
    /// run's stage, for a step of a run, and over that the stage `step` opens, as a person
    /// walks there; going back walks the same way. None for a step no stage takes.
    pub fn opened_on(now: DateTime<Utc>, step: &NextStep) -> Option<(State, Vec<Request>)> {
        let opens = Stage::opened_by(step)?;
        let (mut state, asked) = State::opened(now);
        let run = step
            .run()
            .filter(|_| !matches!(opens, Stage::Run(_)))
            .and_then(|run| Stage::opened_by(&NextStep::Follow { run: run.clone() }));
        state.opened_on = run.into_iter().chain([opens]).collect();
        Some((state, asked))
    }

    /// What `input` does: the state it leaves, and what it asks for.
    pub fn update(&mut self, input: Input) -> Vec<Request> {
        match input {
            Input::Stroke(stroke) => self.pressed(stroke),
            Input::Laid(laid) => {
                if let Some(Dialog::Help(help)) = self.dialog.as_mut() {
                    help.laid(laid.overlay.unwrap_or_default());
                }
                if let Some(stage) = self.stage.as_mut() {
                    stage.laid(laid);
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
                        let wants = stage.heard(news, self.now);
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
                Some(_) => self.answered(outcome),
                None => Vec::new(),
            },
            Input::Unshown(page) => {
                if let Some(stage) = self.stage.as_mut() {
                    stage.unshown(&page);
                }
                Vec::new()
            }
        }
    }

    /// Whether a field is open where the person is, so a surface hands what is pasted
    /// in as typed text: anywhere else a paste answers nothing.
    pub fn typing(&self) -> bool {
        self.dialog.is_none() && self.stage.as_ref().is_some_and(Stage::typing)
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

    /// This moment, as a screen any surface lays out.
    pub fn screen(&self, look: &Look) -> Screen {
        let mut screen = match &self.stage {
            Some(stage) => stage.screen(look, self.now),
            None => self.unopened_screen(),
        };
        match &self.dialog {
            Some(Dialog::Confirmation(confirmation)) => {
                screen.overlay = Some(confirmation.overlay());
            }
            Some(Dialog::Palette(palette)) => screen.overlay = Some(palette.overlay(look)),
            Some(Dialog::Help(help)) => screen.overlay = Some(help.overlay(&screen.legend, look)),
            None if self.stage.as_ref().is_some_and(|stage| !stage.typing()) => {
                let legend = std::mem::take(&mut screen.legend);
                screen.legend = yunta_render::screen::over_every_stage(legend);
            }
            None => {}
        }
        screen
    }

    /// The screen before the application knows where it stands: nothing; the runs
    /// being read once it knows the repository; or what stopped it learning, with
    /// leaving left.
    fn unopened_screen(&self) -> Screen {
        let doc = match (&self.unopened, self.repository) {
            (Some(said), _) => Doc::new().with(Note(said.clone())),
            (None, Some(_)) => {
                Doc::new().with(Note(yunta_render::screen::READING_THE_RUNS.to_string()))
            }
            (None, None) => Doc::new(),
        };
        let mut screen = Screen::new(Region::new(doc));
        screen.trail = vec![yunta_render::screen::YUNTA.to_string()];
        if self.unopened.is_some() {
            screen.legend = yunta_render::screen::leaving_legend();
        }
        screen
    }

    /// What a stroke does: to the dialog standing over the stage, or to the stage; over a
    /// stage where nothing is typed, `/` opens the palette.
    fn pressed(&mut self, stroke: Stroke) -> Vec<Request> {
        if let Some(dialog) = self.dialog.take() {
            return self.at_the_dialog(dialog, stroke);
        }
        if let Some(stage) = self.stage.as_ref().filter(|stage| !stage.typing()) {
            let opened = match stroke {
                palette::OPENS => Some(Dialog::Palette(Palette::over(
                    stage.offered(self.now),
                    stage.showing(),
                ))),
                help::OPENS => Some(Dialog::Help(Help::default())),
                _ => None,
            };
            if opened.is_some() {
                self.dialog = opened;
                return Vec::new();
            }
        }
        let wants = match (self.stage.as_mut(), &self.unopened) {
            (Some(stage), _) => stage.pressed(stroke, self.now),
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

    /// What an answer the state waited on does: where the application opens, or what
    /// the stage makes of it. A repository Yunta was never set up in opens on its setup.
    /// Where it first stands, the stages a step opened it on open over it.
    fn answered(&mut self, outcome: Outcome) -> Vec<Request> {
        let wants = match (outcome, self.stage.as_mut()) {
            (Outcome::Repository(Repository::Absent), _) => self.stands_on(Stage::Signpost),
            (Outcome::Repository(Repository::NeverSetUp), _) => {
                self.stands_on(Stage::Setup(SetupStage::new()))
            }
            (Outcome::Repository(repository), _) => {
                self.repository = Some(repository);
                vec![Want::Work(Operation::Field {
                    reach: FieldReach::Here,
                })]
            }
            (Outcome::Field { reading, storage }, None | Some(Stage::Signpost)) => {
                let first_run = self.repository == Some(Repository::SetUp)
                    && reading.rows.is_empty()
                    && reading.unreadable.is_empty();
                let stage = match first_run {
                    true => Stage::FirstRun(FirstRunStage::new()),
                    false => Stage::Field(FieldStage::read(reading, storage, FieldReach::Here)),
                };
                self.stands_on(stage)
            }
            (Outcome::Refused(refused), None) => {
                self.unopened = Some(refused.to_string());
                Vec::new()
            }
            (Outcome::Failed(said), None) => {
                self.unopened = Some(said);
                Vec::new()
            }
            (outcome, Some(stage)) => stage.answered(outcome),
            (_, None) => Vec::new(),
        };
        self.wanted(wants)
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
    /// a palette's row taken by its key or by opening its stage; otherwise it stays.
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
                    Some(stage) => stage.pressed(key, self.now),
                    None => Vec::new(),
                },
                Chosen::Opened(stage) => vec![Want::Open(*stage)],
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
                Want::Show(page) => asked.push(Request::ShowPage(page)),
                Want::Confirm(confirmation) => {
                    self.dialog = Some(Dialog::Confirmation(confirmation));
                }
                Want::Open(stage) => asked.extend(self.opened_over(stage)),
                Want::Replace(stage) => asked.extend(self.replaced_by(stage)),
                Want::Back => asked.extend(self.back()),
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
        let returned = below.returned();
        self.stage = Some(below);
        asked.extend(self.wanted(returned));
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
