//! A state driven with its performer as a surface drives it, with no terminal and no
//! window: each request carried out, each answer handed back, and the strokes a test
//! presses once the state waits on nothing and stands caught up with the log.
//!
//! A following has no ticket to wait on: it is never let go, so nothing tells the state
//! its news has all come. A helper waits on the log instead, reading each run's last seq
//! in the store the performer follows and taking inputs until the state reads what it
//! follows at those heads ([`State::caught_up`]).
//!
//! Where the work's answers and the following's news come back is [`Inputs`]: a screen keeps
//! one for its life and drives every key through it ([`driven_through`]), so the performer
//! makes its following once and follows the run on from there, as an application's does.
//! Whatever already came is folded before the state is read at rest or a key is pressed, as
//! a surface folds what came before it draws: the events a look hands and what the run waits
//! on after them are read together where they came together. A drive handed no inputs reads
//! its own ([`driven`]): what it follows is followed anew for it, read whole.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use yunta_api::Followed;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Performer, Request, State};

/// How long a state waiting on no work takes at most to stand caught up with the log: its
/// following looks a tenth of a second apart and reads what a run waits on beside, so only
/// a state that never catches up waits half a minute.
const CATCHING_UP: Duration = Duration::from_secs(30);

/// Where a performer hands back a state's work and what it follows. Kept by a screen for its
/// life, the performer's one following hands its news here from the first request to follow
/// on. Not the performer's following, which the performer holds.
pub struct Inputs {
    sender: mpsc::UnboundedSender<Input>,
    received: mpsc::UnboundedReceiver<Input>,
}

impl Inputs {
    /// Inputs nothing was handed to yet.
    pub fn new() -> Inputs {
        let (sender, received) = mpsc::unbounded_channel();
        Inputs { sender, received }
    }
}

impl Default for Inputs {
    fn default() -> Inputs {
        Inputs::new()
    }
}

/// A state driven to rest, as [`driven`] leaves it: the state, what only a surface does
/// that it asked for, and each piece of work it handed the performer, in the order asked.
pub struct Driven {
    pub state: State,
    pub surfaced: Vec<Request>,
    pub performed: Vec<Operation>,
}

/// `state` driven with `performer`: what `asked` asks for and every request it leads to
/// carried out, each answer handed back, then each of `strokes` in turn once the state
/// waits on nothing and stands caught up with the log; the state as it stands then with
/// no stroke left, with what only a surface does, showing a page or leaving, that it asked
/// for. What it follows is asked for again first, to the inputs read here: a state handed
/// in may have asked for it of no performer, or of this one for inputs read no longer.
pub async fn drive(
    state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    strokes: &[Stroke],
) -> (State, Vec<Request>) {
    let Driven {
        state, surfaced, ..
    } = driven(state, asked, performer, strokes).await;
    (state, surfaced)
}

/// `state` driven with `performer` as [`drive`] drives it, with each piece of work it handed
/// the performer meanwhile: what a test counts where the work leaves no trace of its own,
/// as a reading does.
pub async fn driven(
    state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    strokes: &[Stroke],
) -> Driven {
    driven_through(&mut Inputs::new(), state, asked, performer, strokes).await
}

/// `state` driven with `performer` as [`driven`] drives it, through `inputs`, which the
/// performer's following hands its news to from the first drive through them on: what it
/// follows asked for again, which a following handing to them already follows on
/// unchanged, and what came there since the last drive folded before the state is read.
pub async fn driven_through(
    inputs: &mut Inputs,
    mut state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    strokes: &[Stroke],
) -> Driven {
    let mut strokes = strokes.iter().copied();
    let again = following(&state).map(Request::Follow);
    let mut asked: Vec<Request> = again.into_iter().chain(asked).collect();
    let mut surfaced = Vec::new();
    let mut performed = Vec::new();
    loop {
        for request in asked.drain(..) {
            if let Request::Perform(_, operation) = &request {
                performed.push(operation.clone());
            }
            surfaced.extend(performer.take(request, &inputs.sender));
        }
        // What already came is folded before the state is read, as a surface folds what
        // came before it draws.
        if let Ok(input) = inputs.received.try_recv() {
            asked = state.update(input);
            continue;
        }
        let at_rest = state.waiting_on().is_empty();
        if at_rest && caught_up(&state, performer) {
            match strokes.next() {
                Some(stroke) => asked = state.update(Input::Stroke(stroke)),
                None => {
                    return Driven {
                        state,
                        surfaced,
                        performed,
                    }
                }
            }
            continue;
        }
        let input = match at_rest {
            true => tokio::time::timeout(CATCHING_UP, inputs.received.recv())
                .await
                .unwrap_or_else(|_| {
                    let followed = state.followed();
                    panic!("the state never stands caught up following {followed:?}")
                }),
            false => inputs.received.recv().await,
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
