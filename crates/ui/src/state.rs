//! See [`crate`]. The application as one value: where the person is, what stands over
//! it, the work it waits on, and the instant it stands at. An input changes it and
//! says what it asks for; the screen is read off it.
//!
//! The state hands out a [`Ticket`] for each piece of work it asks for and keeps the
//! ones it waits on, each with who asked it. An answer under a ticket it no longer waits on
//! is dropped, so leaving a stage is never undone by a late answer. Work already under way is
//! not asked twice, and a reading asked again lets go the one of its kind its asker waits on.
//! What work hands back goes to the stage that asked it, wherever it stands (`routed`).
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
//! A stage opens over another, which the person comes back to: the stage under it goes on
//! waiting on its work, whose answers land on its page. Leaving a stage for good, going
//! back from it, down past it or another taking its place, lets go the readings it waits on,
//! and what the performer holds for it, while an act goes on and what it comes to is noted
//! where the person is. A stroke goes to the dialog standing over the stage when there is one,
//! and to the stage otherwise.
//!
//! Going down to the field, as `q` does from a run and from every page, ends on the nearest
//! field under the stage the person is at, which notes what leaving said; where no field
//! stands under it, on the stage at the bottom of what stands (the setup in a repository
//! never set up, the signpost outside one, the first request), which comes back into view
//! as any stage does: going down builds no stage and drops none at the bottom. In a
//! repository set up, the stage at the bottom under the one the person is at, the first
//! request or the setup written there, gives way to the field once a run is started from
//! it and once the field holds a run; the first request does so too whenever a person comes
//! back to it. In a repository never set up, and outside one, nothing drops the stage at the
//! bottom.
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
use crate::stage::{FieldStage, SetupStage, StartStage, Want};
use crate::{
    FollowedRuns, Input, Operation, Outcome, RepositoryReading, Request, Room, Stage, Ticket,
};
use routed::{Asker, StageId, Standing, Waited};

mod attending;
mod calling;
mod followed;
mod routed;
mod screen;
mod wanted;

/// The application as one value.
#[derive(Debug, Clone)]
pub struct State {
    /// Where the person is; nothing until the application knows where it stands.
    stage: Option<Standing>,
    /// The stages the one the person is at was opened over, the nearest last.
    under: Vec<Standing>,
    /// What stands over the stage until it is answered.
    dialog: Option<Dialog>,
    /// What stopped the application learning where it stands, where something did.
    unopened: Option<String>,
    /// The stages a step opened the application on, opened in order over where it
    /// first stands, as a person walks there; none once they are.
    opened_on: Vec<Stage>,
    /// The work the state asked for and waits on, by its ticket, with who asked it.
    waiting: BTreeMap<Ticket, Waited>,
    /// The ticket the next piece of work goes under.
    next: Ticket,
    /// Who the next stage to stand is.
    next_stage: StageId,
    /// The instant the application stands at.
    now: DateTime<Utc>,
    /// What it follows, as it last asked, and what it keeps of it.
    followed_runs: FollowedRuns,
    /// Whether the surface has the room it lays the screen out in: short of it, only leaving
    /// answers a key.
    room: Room,
    /// Which runs it keeps need the person, so a run that turns to need them calls them.
    calling: calling::Calling,
    /// The new run a person went back from, left as it was, which the next new run opens on
    /// until a run starts; none where none was left so.
    unstarted: Option<StartStage>,
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
            waiting: BTreeMap::new(),
            next: Ticket::first(),
            next_stage: StageId::first(),
            now,
            followed_runs: FollowedRuns::new(),
            room: Room::Enough,
            calling: calling::Calling::default(),
            unstarted: None,
        };
        let asked = state.asked(Operation::ReadRepository, Asker::TheApplication);
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
        asked.extend(self.the_field_at_the_bottom(false));
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
                if let Some(standing) = self.stage.as_mut() {
                    standing.stage.laid(laid, &self.followed_runs, self.now);
                }
                Vec::new()
            }
            Input::Room(room) => {
                self.room = room;
                Vec::new()
            }
            Input::Remark(ticket, remark) => {
                if let Some(waited) = self.waiting.get(&ticket).cloned() {
                    self.remark_handed(&waited, remark);
                }
                Vec::new()
            }
            Input::News(ticket, news) => match self.waiting.get(&ticket).cloned() {
                Some(waited) => self.news_handed(&waited, news),
                None => Vec::new(),
            },
            Input::Time(now) => {
                self.now = now;
                Vec::new()
            }
            Input::Outcome(ticket, outcome) => match self.waiting.remove(&ticket) {
                Some(waited) => self.answered(waited, outcome),
                None => Vec::new(),
            },
            Input::FollowNews(news) => self.folded(news),
            Input::LookFailed(why) => {
                self.followed_runs.look_failed = Some(why);
                Vec::new()
            }
            Input::Unshown(address) => {
                if let Some(standing) = self.stage.as_mut() {
                    standing.stage.unshown(&address);
                }
                Vec::new()
            }
        }
    }

    /// Whether a field is open where the person is, so a surface hands what is pasted
    /// in as typed text: anywhere else a paste answers nothing.
    pub fn typing(&self) -> bool {
        let typing = |stage: &Stage| stage.typing(&self.followed_runs, self.now);
        self.dialog.is_none() && self.stage().is_some_and(typing)
    }

    /// The stages a step opened the application on, yet to open, outermost first: they
    /// stand nowhere on screen until the application first stands somewhere, so two
    /// states that show the same screen and wait on the same work can still differ by them.
    pub fn stages_to_open(&self) -> &[Stage] {
        &self.opened_on
    }

    /// The stage the person is at; none before the application knows where it stands.
    pub fn stage(&self) -> Option<&Stage> {
        self.stage.as_ref().map(|standing| &standing.stage)
    }

    /// The work the screen the person is at says is under way, as it was asked, in a note it
    /// stands on while it waits: what the state waits on while it says so. None before the
    /// application stands anywhere, whose screen waits on what it follows.
    pub fn shown_under_way(&self) -> Option<Operation> {
        self.stage().and_then(Stage::shown_under_way)
    }

    /// The stages the one the person is at was opened over, outermost first, the nearest
    /// last: where going back leads.
    pub fn stages_under(&self) -> impl DoubleEndedIterator<Item = &Stage> + ExactSizeIterator {
        self.under.iter().map(|standing| &standing.stage)
    }

    /// What stands over the stage until it is answered, where something does.
    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }

    /// The work the state waits on, each under its ticket, in the order it was asked.
    pub fn waiting_on(&self) -> Vec<(Ticket, Operation)> {
        self.waiting
            .iter()
            .map(|(ticket, waited)| (*ticket, waited.operation.clone()))
            .collect()
    }

    /// What a stroke does: to the dialog standing over the stage, or to the stage; over a
    /// stage where nothing is typed, `/` opens the palette.
    fn pressed(&mut self, stroke: Stroke) -> Vec<Request> {
        if self.room == Room::Short {
            let by = self.on_screen();
            let wants = match stroke {
                Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            };
            return self.wanted(by, wants);
        }
        if let Some(dialog) = self.dialog.take() {
            return self.at_the_dialog(dialog, stroke);
        }
        let (followed, now) = (&self.followed_runs, self.now);
        if let Some(stage) = self
            .stage
            .as_mut()
            .map(|standing| &mut standing.stage)
            .filter(|stage| !stage.typed(followed, now))
        {
            let opened = match stroke {
                palette::OPENS => Some(Dialog::Palette(Palette::over(
                    stage.offered(&self.followed_runs, self.now),
                    Stage::always(&self.followed_runs),
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
        let by = self.on_screen();
        // Before it stands anywhere, what stopped it learning where, or a look at the field
        // that failed, leaves leaving to the person.
        let stopped = self.unopened.is_some() || self.followed_runs.look_failed.is_some();
        let wants = match (self.stage.as_mut(), stopped) {
            (Some(standing), _) => (standing.stage).pressed(stroke, &self.followed_runs, self.now),
            (None, true) => match stroke {
                Stroke::Insert('q') | Stroke::Decline | Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
            (None, false) => match stroke {
                Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
        };
        self.wanted(by, wants)
    }

    /// What an answer to the work `waited` was does: a run handed to a detached engine,
    /// started, resumed or answered, kept as handed on until a reading of that engine comes; a
    /// setup written makes the repository one a run can be asked for in; a run started is kept
    /// on its way into the field until the field holds it, lets go the new run kept unstarted,
    /// and the stage at the bottom gives way to the field of this repository, which holds a run
    /// from then on; then the answer handed to who asked it (`routed`).
    fn answered(&mut self, waited: Waited, outcome: Outcome) -> Vec<Request> {
        if let Some(run) = outcome.handed_on() {
            self.followed_runs.handed_on(run.clone());
        }
        if matches!(outcome, Outcome::Initialized { .. }) {
            self.followed_runs.set_up();
        }
        let mut asked = Vec::new();
        if let Outcome::Started { run } = &outcome {
            self.followed_runs.started(run.clone());
            self.unstarted = None;
            asked.extend(self.the_field_at_the_bottom(true));
        }
        asked.extend(self.answer_handed(waited, outcome));
        asked
    }

    /// What an answer to the application's own work does, the repository's reading: where
    /// the application opens, by what was read; what refused or failed it said, and leaving
    /// left. A repository Yunta was never set up in opens on its setup.
    fn opened_answered(&mut self, outcome: Outcome) -> Vec<Request> {
        match outcome {
            Outcome::Repository(read) => self.read_in(*read),
            Outcome::Refused(refused) => {
                self.unopened = Some(refused.to_string());
                Vec::new()
            }
            Outcome::Failed(said) => {
                self.unopened = Some(said);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// What the repository read as `read` does: where the project keeps its runs, its
    /// prices, the repository and the person's home kept for every stage, then outside a
    /// repository the signpost, in one never set up its setup, and in one set up nowhere yet:
    /// the field here is followed, and its first news says where it stands.
    fn read_in(&mut self, read: RepositoryReading) -> Vec<Request> {
        self.followed_runs.storage = Some(read.storage);
        self.followed_runs.prices = read.prices;
        self.followed_runs.home = read.home;
        self.followed_runs.repository = Some(read.repository.clone());
        match read.repository {
            Repository::Absent => self.stands_on(Stage::Signpost),
            Repository::NeverSetUp { .. } => self.stands_on(Stage::Setup(SetupStage::new())),
            Repository::SetUp { .. } => Vec::new(),
        }
    }

    /// The application standing on `stage`, where it first stands: what `stage` asks as
    /// it opens, then each stage a step opened the application on, opened over it.
    fn stands_on(&mut self, stage: Stage) -> Vec<Request> {
        let standing = self.stood(stage);
        let (by, opening) = (Asker::Stage(standing.id), standing.stage.opening());
        self.stage = Some(standing);
        let mut asked = self.wanted(by, opening);
        let walked = std::mem::take(&mut self.opened_on);
        let walked = walked.into_iter().map(Want::Open).collect();
        asked.extend(self.wanted(Asker::TheApplication, walked));
        asked
    }

    /// In a repository set up, the stage at the bottom under the one the person is at, the
    /// first request or the setup written there, given way to the field of this repository,
    /// where a run was `started` here or the field holds a run: it leaves as any stage leaves.
    /// Nothing in a repository never set up nor outside one, where nothing drops the stage at
    /// the bottom, nor where the person is at the bottom, which stands until their key.
    fn the_field_at_the_bottom(&mut self, started: bool) -> Vec<Request> {
        let holds_a_run =
            (self.followed_runs.field.as_ref()).is_some_and(|field| field.holds_a_run());
        let gives_way =
            |standing: &Standing| matches!(standing.stage, Stage::FirstRun(_) | Stage::Setup(_));
        let at_the_bottom = self.under.first().is_some_and(gives_way);
        if !(started || holds_a_run) || !at_the_bottom || !self.followed_runs.can_ask_for_a_run() {
            return Vec::new();
        }
        let field = Stage::Field(FieldStage::reaching(FieldReach::Here));
        let field = self.stood(field);
        let left = std::mem::replace(&mut self.under[0], field);
        self.gone(&left)
    }

    /// What `stroke` does at `dialog`, standing over the stage: answered, it closes and
    /// what was chosen is done, a confirmed act, or the act going back from it is, asked for
    /// by the stage that wanted it, and a palette's row taken by its key or as its step is;
    /// otherwise it stays.
    fn at_the_dialog(&mut self, dialog: Dialog, stroke: Stroke) -> Vec<Request> {
        let by = self.on_screen();
        let wants = match dialog {
            Dialog::Confirmation(confirmation) => match confirmation.pressed(stroke) {
                Confirmed::Yes(operation) | Confirmed::No(Some(operation)) => {
                    match self.stage.as_mut() {
                        Some(standing) => standing.stage.confirmed(operation),
                        None => vec![Want::Work(operation)],
                    }
                }
                Confirmed::No(None) => Vec::new(),
                Confirmed::Leave => vec![Want::Leave],
                Confirmed::Stay => {
                    self.dialog = Some(Dialog::Confirmation(confirmation));
                    Vec::new()
                }
            },
            Dialog::Palette(mut palette) => match palette.pressed(stroke) {
                Chosen::Pressed(key) => match self.stage.as_mut() {
                    Some(standing) => standing.stage.pressed(key, &self.followed_runs, self.now),
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
        self.wanted(by, wants)
    }
}

#[cfg(test)]
mod tests;
