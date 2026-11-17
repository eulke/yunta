//! A state driven with its performer as a surface drives it, with no terminal and no
//! window: each request carried out, each answer handed back, and the strokes a test
//! presses once the state waits on nothing and stands caught up with the log.
//!
//! A following has no ticket to wait on: it is never let go, so nothing tells the state
//! its news has all come. A helper waits on the log instead, reading each run's last seq
//! in the store the performer follows and taking inputs until the state reads what it
//! follows at those heads ([`State::caught_up`]).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use yunta_api::Followed;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Performer, Request, State};

/// How long a state waiting on no work takes at most to stand caught up with the log: its
/// following looks a tenth of a second apart and reads what a run waits on beside, so only
/// a state that never catches up waits half a minute.
const CATCHING_UP: Duration = Duration::from_secs(30);

/// `state` driven with `performer`: what `asked` asks for and every request it leads to
/// carried out, each answer handed back, then each of `strokes` in turn once the state
/// waits on nothing and stands caught up with the log; the state as it stands then with
/// no stroke left, with what only a surface does, showing a page or leaving, that it asked
/// for. What it follows is asked for again first, to the inputs read here: a state handed
/// in may have asked for it of no performer, or of this one for inputs read no longer.
pub async fn drive(
    mut state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    strokes: &[Stroke],
) -> (State, Vec<Request>) {
    let (inputs, mut received) = mpsc::unbounded_channel();
    let mut strokes = strokes.iter().copied();
    let again = following(&state).map(Request::Follow);
    let mut asked: Vec<Request> = again.into_iter().chain(asked).collect();
    let mut surfaced = Vec::new();
    loop {
        for request in asked.drain(..) {
            surfaced.extend(performer.take(request, &inputs));
        }
        let at_rest = state.waiting_on().is_empty();
        if at_rest && caught_up(&state, performer) {
            match strokes.next() {
                Some(stroke) => asked = state.update(Input::Stroke(stroke)),
                None => return (state, surfaced),
            }
            continue;
        }
        let input = match at_rest {
            true => tokio::time::timeout(CATCHING_UP, received.recv())
                .await
                .unwrap_or_else(|_| {
                    let followed = state.followed();
                    panic!("the state never stands caught up following {followed:?}")
                }),
            false => received.recv().await,
        };
        asked = state.update(input.expect("the performer hands back what the state waits on"));
    }
}

/// What `state` follows, where it follows anything.
fn following(state: &State) -> Option<Followed> {
    let followed = state.followed();
    (followed.field.is_some() || followed.run.is_some()).then_some(followed)
}

/// Whether `state` reads what it follows at each run's last seq in the store `performer`
/// follows: at once where it follows nothing, which reads no store.
fn caught_up(state: &State, performer: &Performer) -> bool {
    if following(state).is_none() {
        return true;
    }
    let project = performer.workspace().project();
    let project = project.expect("the performer's project names the store it follows");
    state.caught_up(&yunta_testkit::heads(&project.storage_path))
}
