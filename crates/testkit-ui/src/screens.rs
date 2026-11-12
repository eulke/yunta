//! The states the terminal application's target frames show, each the same on every
//! machine: the application driven against runs a bench makes, with fixed ids, at the
//! instant every test stands at, and a person's keys pressed on it.
//!
//! - [`field`]: a repository's runs, one finished and one parked on its gate, which the
//!   attention row names;
//! - [`run`]: the run parked on its gate, its ribbon across the screen, its chronicle, and
//!   the page of its gate beside it;
//! - [`card`]: the run's decision, its card put to the person;
//! - [`desk`]: the plan it is on, read at its desk;
//! - [`confirmation`]: a run's stop asked over the field;
//! - [`palette`] and [`help`], each over the run.
//!
//! And [`walked_to`], where a person's keys take them from the field, beside
//! [`reviewed`]: the application opened on `yunta review` of the parked run, which stands
//! where a person walks to the plan, and goes back the way they walked.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use yunta_core::step::NextStep;
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::Bench;
use yunta_testkit_core::fixed_now;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Performer, State};

use crate::driving::drive;
use crate::sandbox::workspace;

/// The run parked on its plan's gate, by its fixed id.
const PARKED: &str = "01M7SCREEN0000000000Q78GRR";

/// How long after its runs were made the application stands, in seconds: a run that has
/// waited a few minutes, as the screens' reference shows one.
const SECONDS_AFTER_THE_RUNS: i64 = 454;

/// A mock fixture with no sessions.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The application opened on the repository's runs, then `strokes` pressed, each once
/// the state waits on nothing: where a person walks to with them.
pub async fn walked_to(strokes: &[Stroke]) -> State {
    World::made().await.driven(strokes).await
}

/// The application open on a repository's runs: one finished, and one parked on its gate.
pub async fn field() -> State {
    walked_to(&[]).await
}

/// The run parked on its gate, opened from the field, the page of its gate beside it: a
/// node that produced no file, so the page names no path of the bench's.
pub async fn run() -> State {
    walked_to(&[Stroke::Enter, Stroke::Down, Stroke::Enter]).await
}

/// The run's decision, its card put to the person.
pub async fn card() -> State {
    walked_to(&[Stroke::Enter, Stroke::Insert('a')]).await
}

/// The plan the run's decision is on, read at its desk.
pub async fn desk() -> State {
    walked_to(&[Stroke::Enter, Stroke::Insert('a'), Stroke::Insert('r')]).await
}

/// The application opened on `yunta review` of the parked run, its work answered, then
/// `strokes` pressed: where `review` opens it, over the run and the field.
pub async fn reviewed(strokes: &[Stroke]) -> State {
    let run = PARKED.parse().expect("the parked run's id");
    World::made()
        .await
        .driven_from(&NextStep::Review { run }, strokes)
        .await
}

/// Stopping the parked run, asked over the field.
pub async fn confirmation() -> State {
    walked_to(&[Stroke::Insert('c')]).await
}

/// The palette, over the run.
pub async fn palette() -> State {
    walked_to(&[Stroke::Enter, Stroke::Insert('/')]).await
}

/// Help, over the run.
pub async fn help() -> State {
    walked_to(&[Stroke::Enter, Stroke::Insert('?')]).await
}

/// A repository whose runs a bench made, and the application's work answered there.
struct World {
    parked: Bench,
    _finished: Bench,
}

impl World {
    /// The repository's two runs, each with its fixed id, made by its own bench beside the
    /// other.
    async fn made() -> World {
        let parked = Bench::with_run_id(PARKED);
        crate::planned::parked_on_its_plan(&parked).await;
        let finished = parked.beside("01M7SCREEN0000000000EKPBGE");
        finished.run(FINISHES, NO_SESSIONS).await;
        World {
            parked,
            _finished: finished,
        }
    }

    /// The application opened in the repository, its work answered, then `strokes`
    /// pressed, each once the state waits on nothing.
    async fn driven(&self, strokes: &[Stroke]) -> State {
        let root: &Path = self
            .parked
            .store_path()
            .parent()
            .expect("a bench's log stands in its root");
        let performer = Arc::new(Performer::new(workspace(&self.parked.worktree, root)));
        let (opened, asked) = State::opened(stands_at());
        drive(opened, asked, &performer, strokes).await.0
    }

    /// The application opened in the repository on `step`, its work answered, then
    /// `strokes` pressed, each once the state waits on nothing.
    async fn driven_from(&self, step: &NextStep, strokes: &[Stroke]) -> State {
        let root: &Path = self
            .parked
            .store_path()
            .parent()
            .expect("a bench's log stands in its root");
        let performer = Arc::new(Performer::new(workspace(&self.parked.worktree, root)));
        let (opened, asked) = State::opened_on(stands_at(), step).expect("a stage takes the step");
        drive(opened, asked, &performer, strokes).await.0
    }
}

/// The instant the screens stand at: some minutes after the bench made the runs.
fn stands_at() -> DateTime<Utc> {
    fixed_now() + Duration::seconds(SECONDS_AFTER_THE_RUNS)
}
