//! The states the terminal application's target frames show, each the same on every
//! machine: the application driven against runs a bench makes, with fixed ids, at the
//! instant every test stands at, and a person's keys pressed on it.
//!
//! - [`walked_to`]: a repository's runs, one finished and one parked on its gate, which the
//!   attention row names, and where a person's keys take them from there; beside it
//!   [`reviewed`], the application opened on `yunta review` of the parked run, the run's
//!   screen on its plan over the field. [`World`] is the repository they stand in, for a test
//!   that reads its log beside a state.
//! - [`mod@run`]: a run's screen as a target's cut stands, by the target's name, the
//!   design's run or a small run in place of a bench's: what every test of a scene builds
//!   its state from.
//! - [`mod@field`]: the field as a target's cut stands, by the target's name: the runs it
//!   shows loaded into one store, the application opened on them in the cut's repository.
//! - [`mod@home`]: a screen a target opens in a place of the test home, by the target's
//!   name: the place made alone, the application opened there.
//!
//! Where a screen's state names the place it was made in, a fresh directory on each machine,
//! the application is handed it as the target's frames name it, as the dumps they were
//! written from do (`named_as`): a cut's place as `/cut`, a place of the home under its root
//! as `/home`.

pub mod field;
pub mod home;
pub mod run;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use yunta_api::{FollowNews, Repository};
use yunta_core::events::Channel;
use yunta_core::step::NextStep;
use yunta_core::{RunId, Seq};
use yunta_render::page::{RunUnread, UnreadableRun};
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::Bench;
use yunta_testkit_core::fixed_now;
use yunta_ui::prompt::Stroke;
use yunta_ui::{EvidenceOutcome, Input, Outcome, Performer, SetupPackFetch, State};

use crate::cut::{TargetCut, TARGET_CUTS};
use crate::driving::drive;
use crate::sandbox::workspace;

/// The run parked on its plan's gate, by its fixed id.
const PARKED: &str = "01M7SCREEN0000000000Q78GRR";

/// What a target names the place a cut is kept under, as the dumps it was written from name
/// it.
const CUT: &str = "/cut";

/// How long after its runs were made the application stands, in seconds: a run that has
/// waited a few minutes, as the screens' reference shows one.
const SECONDS_AFTER_THE_RUNS: i64 = 454;

/// A mock fixture with no sessions.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The application opened on the repository's runs, then `strokes` pressed, each once
/// the state waits on nothing and stands caught up with the log: where a person walks to
/// with them.
pub async fn walked_to(strokes: &[Stroke]) -> State {
    World::made().await.driven(strokes).await
}

/// The application opened on `yunta review` of the parked run, its work answered, then
/// `strokes` pressed: where `review` opens it, the run's screen on its plan, over the field.
pub async fn reviewed(strokes: &[Stroke]) -> State {
    let run = PARKED.parse().expect("the parked run's id");
    World::made()
        .await
        .driven_from(&NextStep::Review { run }, strokes)
        .await
}

/// A repository whose runs a bench made, one finished and one parked on its plan's gate,
/// and the application's work answered there.
pub struct World {
    parked: Bench,
    _finished: Bench,
}

impl World {
    /// The repository's two runs, each with its fixed id, made by its own bench beside the
    /// other.
    pub async fn made() -> World {
        let parked = Bench::with_run_id(PARKED);
        crate::planned::parked_on_its_plan(&parked).await;
        let finished = parked.beside("01M7SCREEN0000000000EKPBGE");
        finished.run(FINISHES, NO_SESSIONS).await;
        World {
            parked,
            _finished: finished,
        }
    }

    /// The run parked on its plan's gate.
    pub fn parked(&self) -> &RunId {
        &self.parked.run_id
    }

    /// Each run's last seq in the repository's log.
    pub fn heads(&self) -> BTreeMap<RunId, Seq> {
        yunta_testkit::heads(self.parked.store_path())
    }

    /// The application opened in the repository, its work answered, then `strokes`
    /// pressed, each once the state waits on nothing and stands caught up with the log.
    pub async fn driven(&self, strokes: &[Stroke]) -> State {
        let root: &Path = self
            .parked
            .store_path()
            .parent()
            .expect("a bench's log stands in its root");
        let performer = Arc::new(Performer::new(
            workspace(&self.parked.worktree, root),
            Channel::Tty,
        ));
        let (opened, asked) = State::opened(stands_at());
        drive(opened, asked, &performer, strokes).await.0
    }

    /// The application opened in the repository on `step`, its work answered, then
    /// `strokes` pressed, each once the state waits on nothing and stands caught up with
    /// the log.
    pub async fn driven_from(&self, step: &NextStep, strokes: &[Stroke]) -> State {
        let root: &Path = self
            .parked
            .store_path()
            .parent()
            .expect("a bench's log stands in its root");
        let performer = Arc::new(Performer::new(
            workspace(&self.parked.worktree, root),
            Channel::Tty,
        ));
        let (opened, asked) = State::opened_on(stands_at(), step).expect("a stage takes the step");
        drive(opened, asked, &performer, strokes).await.0
    }
}

/// The instant the screens stand at: some minutes after the bench made the runs.
fn stands_at() -> DateTime<Utc> {
    fixed_now() + Duration::seconds(SECONDS_AFTER_THE_RUNS)
}

/// The row of [`TARGET_CUTS`] of the target `name`.
fn target(name: &str) -> &'static TargetCut {
    TARGET_CUTS
        .iter()
        .find(|target| target.name == name)
        .unwrap_or_else(|| panic!("no target is named `{name}`"))
}

/// `key`, as the design's recording names a key, as the strokes a surface hands in: a
/// letter or a digit, `enter`, `esc`, `tab`, `space`, `up`, `down`, `/`, `?`, or `type:`
/// and the words typed, a stroke a character.
fn strokes(key: &str) -> Vec<Stroke> {
    if let Some(typed) = key.strip_prefix("type:") {
        return typed.chars().map(Stroke::Insert).collect();
    }
    let stroke = match key {
        "enter" => Stroke::Enter,
        "esc" => Stroke::Decline,
        "tab" => Stroke::Tab,
        "space" => Stroke::Insert(' '),
        "up" => Stroke::Up,
        "down" => Stroke::Down,
        one => {
            let mut chars = one.chars();
            match (chars.next(), chars.next()) {
                (Some(char), None) => Stroke::Insert(char),
                _ => panic!("no key is named `{key}`"),
            }
        }
    };
    vec![stroke]
}

/// `input` with `place` named `name` wherever the application is handed it: in where the
/// repository it opened in stands, in why a run of the field cannot be read, in what git
/// said of the pack a setup installs where it is out of reach, and in where a run's page for
/// a browser was written.
fn named_as(input: Input, place: &Path, name: &str) -> Input {
    let named = |path: &Path| match path.strip_prefix(place) {
        Ok(under) => Path::new(name).join(under),
        Err(_) => path.to_path_buf(),
    };
    let unread = |run: UnreadableRun| {
        let problem = match run.problem {
            RunUnread::Manifest { path } => RunUnread::Manifest { path: named(&path) },
            RunUnread::NoDirectory { runs_root } => RunUnread::NoDirectory {
                runs_root: named(&runs_root),
            },
            other => other,
        };
        UnreadableRun { problem, ..run }
    };
    match input {
        Input::Outcome(ticket, Outcome::Repository(mut read)) => {
            read.repository = match read.repository {
                Repository::SetUp { dir } => Repository::SetUp { dir: named(&dir) },
                Repository::NeverSetUp { dir } => Repository::NeverSetUp { dir: named(&dir) },
                Repository::Absent => Repository::Absent,
            };
            Input::Outcome(ticket, Outcome::Repository(read))
        }
        Input::FollowNews(FollowNews::Field(mut field)) => {
            field.unreadable = field.unreadable.into_iter().map(unread).collect();
            Input::FollowNews(FollowNews::Field(field))
        }
        Input::FollowNews(FollowNews::Unreadable(run)) => {
            Input::FollowNews(FollowNews::Unreadable(unread(run)))
        }
        Input::Outcome(ticket, Outcome::SetupPack { fetched, held }) => {
            let fetched = match Arc::try_unwrap(fetched) {
                Ok(SetupPackFetch::OutOfReach { pack, why }) => {
                    let why = why.replace(&place.display().to_string(), name);
                    Arc::new(SetupPackFetch::OutOfReach { pack, why })
                }
                Ok(other) => Arc::new(other),
                Err(shared) => shared,
            };
            Input::Outcome(ticket, Outcome::SetupPack { fetched, held })
        }
        Input::Outcome(ticket, Outcome::Evidence(EvidenceOutcome::PageWritten { page })) => {
            let page = named(&page);
            Input::Outcome(
                ticket,
                Outcome::Evidence(EvidenceOutcome::PageWritten { page }),
            )
        }
        other => other,
    }
}
