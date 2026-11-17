//! When the application calls the person, through `State` alone: a run that comes to need
//! them while the field stands names the window's title and calls them once, and news of
//! the same pause call nothing more; a run that needed them as the application opened
//! calls nobody, though the title names it; and a run answered leaves the window its own
//! title. One following stands throughout, as a surface keeps one, from a performer
//! working in a bench's world.

mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use surco::look::Look;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use yunta_api::Stop;
use yunta_core::events::{EventDraft, EventPayload, HostSuspendedPayload, RunEvent};
use yunta_core::RunId;
use yunta_render::watched;
use yunta_testkit::workflows::GATED;
use yunta_testkit::{Bench, ScriptedInteraction};
use yunta_testkit_core::FixedClock;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::{Input, Performer, Request, State};

use support::{performer_in, stopped_beside};

/// A mock fixture with no sessions: the gated workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// How long a following takes at most to hand a bench's world whole: its looks come a
/// tenth of a second apart, so only a state that never catches up waits half a minute.
const CATCHING_UP: Duration = Duration::from_secs(30);

/// A run stopped as the application opens, which parks on its gate while the field stands.
const PARKS: RunId = RunId::from_static("01M7CA110000000000000PARKS");

/// A run parked on its gate before the application opens.
const NEEDED: RunId = RunId::from_static("01M7CA11000000000000NEEDED");

/// What a surface does for a state, with a performer working in a bench's world: what the
/// state asks carried out, and what the performer hands back handed to the state, through
/// one channel, so one following stands throughout.
struct Surface {
    performer: Arc<Performer>,
    store: PathBuf,
    inputs: UnboundedSender<Input>,
    received: UnboundedReceiver<Input>,
}

impl Surface {
    /// A surface whose performer works in `world`.
    fn over(world: &Bench) -> Surface {
        let (inputs, received) = mpsc::unbounded_channel();
        Surface {
            performer: performer_in(world),
            store: world.store_path().to_path_buf(),
            inputs,
            received,
        }
    }

    /// `asked` carried out, then what the performer hands taken by `state` until it waits
    /// on nothing and stands caught up with the log: every request it asked on the way.
    async fn caught_up(&mut self, state: &mut State, mut asked: Vec<Request>) -> Vec<Request> {
        let mut all = Vec::new();
        loop {
            for request in asked.drain(..) {
                all.push(request.clone());
                self.performer.take(request, &self.inputs);
            }
            let heads = yunta_testkit::heads(&self.store);
            if state.waiting_on().is_empty() && state.caught_up(&heads) {
                return all;
            }
            let came = tokio::time::timeout(CATCHING_UP, self.received.recv());
            let input = came.await.expect("the state catches up with the log");
            asked = state.update(input.expect("the performer hands what it reads"));
        }
    }
}

/// A repository set up, whose runs log in the world's store.
async fn set_up() -> Bench {
    let world = Bench::new();
    let config = world.worktree.join(".yunta/config.yaml");
    let dir = config.parent().expect("the repository's .yunta");
    tokio::fs::create_dir_all(dir).await.expect("its .yunta");
    tokio::fs::write(&config, "defaults:\n  isolation: none\n")
        .await
        .expect("its config");
    world
}

/// The application opened in `surface`'s world and standing on its field, caught up with
/// the log: the state, and every request it asked.
async fn opened(surface: &mut Surface) -> (State, Vec<Request>) {
    let (mut state, asked) = State::opened(now());
    let asked = surface.caught_up(&mut state, asked).await;
    (state, asked)
}

/// How many times `asked` calls the person.
fn calls(asked: &[Request]) -> usize {
    let calling = |request: &&Request| matches!(request, Request::CallThePerson);
    asked.iter().filter(calling).count()
}

/// The window's title `state`'s screen asks for.
fn title(state: &State) -> Option<String> {
    state.screen(&Look::plain()).title
}

/// What the window's title says of `run` while it needs the person, as render words it of
/// the run as the project reads it.
async fn named(world: &Bench, run: &RunId) -> String {
    let root = world
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let project = workspace(&world.worktree, root)
        .project()
        .expect("the project");
    let frame = project.run(run).await.expect("the run").frame();
    watched::title(&frame, run, Look::plain().glyphs)
}

/// An event that moves nothing but the clock of `bench`'s run, appended behind the
/// engine's back: news of the pause it stands at.
fn slept(bench: &Bench) {
    let slept = HostSuspendedPayload::slept(Duration::from_secs(3));
    let payload = EventPayload::Run(RunEvent::HostSuspended(slept));
    let draft = EventDraft::new(bench.run_id.clone(), None, payload).unwrap();
    bench.storage.append(&draft, &FixedClock).unwrap();
}

/// `bench`'s run woken with nothing stopping it, answering its gate with `answer` where
/// one is given, and parking on it where none is.
async fn woken(bench: Bench, answer: Option<&str>) -> Bench {
    let bench = bench.with_cancel(Stop::new().work().clone());
    match answer {
        Some(answer) => {
            bench
                .wake_answering(&ScriptedInteraction::choose(answer))
                .await
        }
        None => bench.wake().await,
    };
    bench
}

/// A run that parks while the field stands names the window's title and calls the person
/// once; the next news of that pause call nothing; answered, the window's title is its
/// own again.
#[tokio::test]
async fn a_run_that_comes_to_need_the_person_calls_them_once_and_names_the_window() {
    let world = set_up().await;
    let stopped = stopped_beside(&world, &PARKS, GATED).await;
    let mut surface = Surface::over(&world);
    let (mut state, asked) = opened(&mut surface).await;
    assert_eq!(calls(&asked), 0, "a stopped run needs nobody: {asked:?}");
    assert_eq!(title(&state), None);

    let parked = woken(stopped, None).await;
    let asked = surface.caught_up(&mut state, Vec::new()).await;
    assert_eq!(title(&state), Some(named(&world, &PARKS).await));
    assert_eq!(calls(&asked), 1, "{asked:?}");

    slept(&parked);
    let asked = surface.caught_up(&mut state, Vec::new()).await;
    assert_eq!(calls(&asked), 0, "news of the same pause: {asked:?}");
    assert_eq!(title(&state), Some(named(&world, &PARKS).await));

    woken(parked, Some("go")).await;
    let asked = surface.caught_up(&mut state, Vec::new()).await;
    assert_eq!(calls(&asked), 0, "{asked:?}");
    assert_eq!(title(&state), None, "the run answered");
}

/// A run that needed the person as the application opened calls nobody: the window's
/// title names it all the same.
#[tokio::test]
async fn a_run_that_needed_the_person_as_the_application_opened_calls_nobody() {
    let world = set_up().await;
    let needed = world.beside(NEEDED.as_str());
    needed.run(GATED, NO_SESSIONS).await;
    let mut surface = Surface::over(&world);
    let (state, asked) = opened(&mut surface).await;
    assert_eq!(calls(&asked), 0, "{asked:?}");
    assert_eq!(title(&state), Some(named(&world, &NEEDED).await));
}
