//! The application driven end to end where its work starts Yunta itself: a state and
//! a performer whose workspace runs this build's binary, with the mock adapter, in a
//! repository a test makes. Each group of stages adds its file beside this one.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use surco::surface::Markdown;
use tokio::sync::mpsc;
use yunta_api::Workspace;
use yunta_core::{Env, ProcessSecrets, SystemIdSource};
use yunta_testkit_core::FixedClock;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Performer, Request, State};

#[path = "interface/decision.rs"]
mod decision;
#[path = "interface/field.rs"]
mod field;
#[path = "interface/questions.rs"]
mod questions;
#[path = "interface/run.rs"]
mod run;
#[path = "interface/start.rs"]
mod start;

/// A workspace working in `dir`, its state root and org layer in `home`, whose detached
/// work runs this build's `yunta`, on a `PATH` git stands on: what the testkit hands
/// the binary it runs.
fn workspace(dir: &Path, home: &Path) -> Workspace {
    let env = Env {
        home: Some(home.to_path_buf()),
        yunta_home: Some(home.to_path_buf()),
        org_config: Some(home.join("org.yaml")),
        path: vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
        user: Some("yunta-test".to_string()),
        ..Env::default()
    };
    Workspace {
        dir: dir.to_path_buf(),
        env,
        clock: Arc::new(FixedClock),
        ids: Arc::new(SystemIdSource),
        secrets: Arc::new(ProcessSecrets),
        binary: PathBuf::from(env!("CARGO_BIN_EXE_yunta")),
    }
}

/// The instant the application stands at in these tests.
fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000, 0).expect("an instant")
}

/// `state` driven with `performer`: what `asked` asks for and every request it leads to
/// carried out, each answer handed back, then each of `strokes` in turn once the state
/// waits on nothing; the state as it stands when it waits on nothing and no stroke is
/// left, with what only a surface does, showing a page or leaving, that it asked for.
async fn drive(
    mut state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    strokes: &[Stroke],
) -> (State, Vec<Request>) {
    let (inputs, mut received) = mpsc::unbounded_channel();
    let mut strokes = strokes.iter().copied();
    let (mut asked, mut surfaced) = (asked, Vec::new());
    loop {
        for request in asked.drain(..) {
            surfaced.extend(performer.take(request, &inputs));
        }
        if state.waiting_on().is_empty() {
            match strokes.next() {
                Some(stroke) => asked = state.update(Input::Stroke(stroke)),
                None => return (state, surfaced),
            }
            continue;
        }
        let input = received
            .recv()
            .await
            .expect("the performer hands back what the state waits on");
        asked = state.update(input);
    }
}

/// Outside a repository the application rests on the signpost and leaves at `q`.
#[tokio::test]
async fn outside_a_repository_it_rests_on_the_signpost() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let performer = Arc::new(Performer::new(workspace(&outside, root.path())));
    let (opened, asked) = State::opened(now());
    let (state, surfaced) = drive(opened, asked, &performer, &[Stroke::Insert('q')]).await;
    assert!(state.waiting_on().is_empty());
    let screen = Markdown.screen(&state.screen(&Markdown.look()));
    assert!(screen.contains("not in a repository"), "{screen}");
    assert_eq!(surfaced, [Request::Leave]);
}
