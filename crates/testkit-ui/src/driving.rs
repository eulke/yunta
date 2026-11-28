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
//!
//! A person also presses keys while work is under way. A drive that holds the work a test
//! picks back from the performer ([`pressed_while`]) presses each key once the state waits on
//! nothing but that work, so every key lands while it is under way and its answer comes after
//! them all: what the test then hands back, the performer's own answer or one of its making.
//! One piece of work is carried out alone, through the application's own path, by
//! [`carried_out`].

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use yunta_api::{Followed, Remark};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, News, Operation, Outcome, Performer, Request, State, Ticket};

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
    /// What each input handed back is made into before the state takes it, where a screen
    /// names something as frames written apart from it name it; none takes each as it is.
    naming: Option<Naming>,
}

/// What an input handed back is made into before a state takes it.
type Naming = Box<dyn Fn(Input) -> Input + Send + Sync>;

impl Inputs {
    /// Inputs nothing was handed to yet, each taken as it is handed.
    pub fn new() -> Inputs {
        let (sender, received) = mpsc::unbounded_channel();
        Inputs {
            sender,
            received,
            naming: None,
        }
    }

    /// Inputs nothing was handed to yet, each made into what `naming` makes of it before the
    /// state takes it: what a screen held to frames written apart from it names as they do,
    /// as the place a cut is kept under, which is a fresh directory on each machine.
    pub fn naming(naming: impl Fn(Input) -> Input + Send + Sync + 'static) -> Inputs {
        Inputs {
            naming: Some(Box::new(naming)),
            ..Inputs::new()
        }
    }

    /// `input` as the state takes it: as `naming` makes it, where it names anything.
    fn taken(&self, input: Input) -> Input {
        match &self.naming {
            Some(naming) => naming(input),
            None => input,
        }
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
    state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    strokes: &[Stroke],
) -> Driven {
    let Withheld { driven, .. } =
        driven_withholding(inputs, state, asked, performer, |_| false, strokes).await;
    driven
}

/// A drive whose keys were pressed while work was under way, as [`pressed_while`] leaves it:
/// the drive, and each piece of work held back from the performer under the ticket the state
/// asked it under, in the order asked. The state still waits on each where no key let it go:
/// a test hands it to the performer (`Request::Perform`) or answers it itself.
pub struct Withheld {
    pub driven: Driven,
    pub withheld: Vec<(Ticket, Operation)>,
}

/// `state` driven with `performer` as [`driven_through`] drives it, through `inputs`, with the
/// work `withheld` picks held back from the performer: each of `strokes` pressed once the state
/// waits on nothing but that work and stands caught up with the log, so every key lands while
/// it is under way and nothing of it comes back before the last. A key that asks for more of
/// such work has it held back too.
pub async fn pressed_while(
    inputs: &mut Inputs,
    state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    withheld: impl Fn(&Operation) -> bool,
    strokes: &[Stroke],
) -> Withheld {
    driven_withholding(inputs, state, asked, performer, withheld, strokes).await
}

/// `state` driven as [`pressed_while`] drives it: what every drive comes to, the work
/// `withheld` picks held back from the performer, none for a drive that holds nothing back.
async fn driven_withholding(
    inputs: &mut Inputs,
    state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    withheld: impl Fn(&Operation) -> bool,
    strokes: &[Stroke],
) -> Withheld {
    let mut strokes = strokes.iter().copied();
    let again = following(&state).map(Request::Follow);
    let mut asked: Vec<Request> = again.into_iter().chain(asked).collect();
    let driven = Driven {
        state,
        surfaced: Vec::new(),
        performed: Vec::new(),
    };
    let mut drive = Withheld {
        driven,
        withheld: Vec::new(),
    };
    loop {
        drive.handed_on(std::mem::take(&mut asked), performer, inputs, &withheld);
        // What already came is folded before the state is read, as a surface folds what
        // came before it draws.
        if let Ok(input) = inputs.received.try_recv() {
            asked = drive.driven.state.update(inputs.taken(input));
            continue;
        }
        let at_rest = drive.waits_on_nothing_else();
        if at_rest && caught_up(&drive.driven.state, performer) {
            match strokes.next() {
                Some(stroke) => asked = drive.driven.state.update(Input::Stroke(stroke)),
                None => return drive,
            }
            continue;
        }
        let input = next_input(inputs, &drive.driven.state, at_rest).await;
        asked = drive.driven.state.update(inputs.taken(input));
    }
}

impl Withheld {
    /// `asked` carried out: the work `withheld` picks held back, any other handed to
    /// `performer`, which hands back to `inputs`, and what only a surface does kept.
    fn handed_on(
        &mut self,
        asked: Vec<Request>,
        performer: &Arc<Performer>,
        inputs: &Inputs,
        withheld: &impl Fn(&Operation) -> bool,
    ) {
        for request in asked {
            match request {
                Request::Perform(ticket, operation) if withheld(&operation) => {
                    self.withheld.push((ticket, operation));
                }
                request => {
                    if let Request::Perform(_, operation) = &request {
                        self.driven.performed.push(operation.clone());
                    }
                    let surfaced = performer.take(request, &inputs.sender);
                    self.driven.surfaced.extend(surfaced);
                }
            }
        }
    }

    /// Whether the state waits on nothing but the work held back.
    fn waits_on_nothing_else(&self) -> bool {
        let held = |ticket: &Ticket| self.withheld.iter().any(|(back, _)| back == ticket);
        let waiting = self.driven.state.waiting_on();
        waiting.iter().all(|(ticket, _)| held(ticket))
    }
}

/// The next input handed back to `inputs` for `state`: within the time a state takes at most
/// to stand caught up where it stands `at_rest`, whenever it comes otherwise.
async fn next_input(inputs: &mut Inputs, state: &State, at_rest: bool) -> Input {
    let input = match at_rest {
        true => tokio::time::timeout(CATCHING_UP, inputs.received.recv())
            .await
            .unwrap_or_else(|_| {
                let followed = state.followed();
                panic!("the state never stands caught up following {followed:?}")
            }),
        false => inputs.received.recv().await,
    };
    input.expect("the performer hands back what the state waits on")
}

/// What a piece of work carried out alone came to: what it remarked and what it found while
/// it lasted, each in the order it said them, then what it came to.
pub struct CarriedOut {
    pub remarks: Vec<Remark>,
    pub news: Vec<News>,
    pub outcome: Outcome,
}

/// `operation` carried out alone by `performer` under `ticket`, handed to it as a surface
/// hands it work ([`Performer::take`]), and what it came to.
pub async fn carried_out(
    performer: &Arc<Performer>,
    ticket: Ticket,
    operation: Operation,
) -> CarriedOut {
    let (sender, mut received) = mpsc::unbounded_channel();
    let surfaced = performer.take(Request::Perform(ticket, operation), &sender);
    assert!(surfaced.is_none(), "work is the performer's: {surfaced:?}");
    let (mut remarks, mut news) = (Vec::new(), Vec::new());
    loop {
        match received.recv().await {
            Some(Input::Remark(_, remark)) => remarks.push(remark),
            Some(Input::News(_, found)) => news.push(found),
            Some(Input::Outcome(_, outcome)) => {
                return CarriedOut {
                    remarks,
                    news,
                    outcome,
                }
            }
            Some(_) => continue,
            None => panic!("the performer handed back no outcome"),
        }
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
