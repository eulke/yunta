//! The walk: from the application as it opens, every stroke, what a terminal of a few
//! rows laid out of the screen, each page a surface could not show, and every recorded
//! piece of news and answer of each piece of work a state waits on, applied to every
//! state it reaches, each state once by its
//! screen and the kinds of work it waits on. Its clock stands still at one instant: what
//! time changes on a screen is each stage's own tests'.
//!
//! The walk holds only what every walk needs: that it ends, that no input panics, and
//! that every piece of work a state asks for is one the recordings answer. What a surface
//! holds of every state it reaches is a [`Checks`] of its own: the interface's rules, a
//! terminal's layout. A state that breaks one is named with the inputs that reach it
//! ([`Reached`]).

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::{catch_unwind, AssertUnwindSafe};

use surco::doc::Block;
use surco::surface::Markdown;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Request, State};

use crate::recorded::Recordings;
use crate::sandbox::now;
use crate::terminal::laid_out;

/// The most states the walk visits before it calls the application endless: more than
/// twice what its stages reach, counted in states rather than seconds so the bound holds
/// alike on a fast machine and on a slow runner beside the rest of the suite.
const MOST_STATES: usize = 40_000;

/// Each character a stage binds, and one no stage binds.
const KEYS: [char; 25] = [
    'q', 'j', 'k', ' ', 'r', 'c', 'x', 'a', 'd', 'f', 'w', 'p', 'n', 't', 'i', 'u', 'e', 'v', 'o',
    's', 'g', 'h', 'm', '1', 'z',
];

/// The most pieces of work a state may wait on for the walk to try strokes on it. A
/// stroke does the same whatever is under way; what turns on the work under way is
/// asking more beside it, which a state waiting on one already meets. A state waiting
/// on more is answered until it waits on one.
const STROKED_WHILE_WAITING: usize = 1;

/// What a surface holds of what the walk reaches. Not the walk's own rules, which every
/// walk holds: a surface's.
pub trait Checks {
    /// `state`, which the walk reached first as `reached` says, its screen as Markdown
    /// draws it.
    fn reached(&mut self, state: &State, markdown: &str, reached: &Reached<'_>);

    /// `input`, handed to `from` as `reached` says, which asked `asked`.
    fn handed(&mut self, from: &State, input: &Input, asked: &[Request], reached: &Reached<'_>);
}

/// How the walk came to where a check stands: the inputs from the application's opening,
/// one a line, read only when a check fails. Not a state: the way to one.
pub struct Reached<'w> {
    steps: &'w [Step],
    last: Option<usize>,
    then: Vec<String>,
}

impl Reached<'_> {
    /// The inputs, first to last, one a line.
    pub fn inputs(&self) -> String {
        let mut inputs: Vec<&str> = self.then.iter().rev().map(String::as_str).collect();
        let mut at = self.last;
        while let Some(step) = at {
            inputs.push(self.steps[step].input.as_str());
            at = self.steps[step].from;
        }
        inputs.reverse();
        inputs.join("\n")
    }

    /// This, then `input`: how a check names a step it takes off the walk.
    pub fn then(&self, input: impl Into<String>) -> Reached<'_> {
        let mut then = self.then.clone();
        then.push(input.into());
        Reached {
            steps: self.steps,
            last: self.last,
            then,
        }
    }
}

/// How the walk reached a state: the input, and the state it was handed to, by its
/// place among the steps; nothing before the application opens.
struct Step {
    from: Option<usize>,
    input: String,
}

/// The walk over what `recordings` answer. Not the states it reaches: it keeps a
/// fingerprint of each and the step that first reached it.
pub struct Walk<'r> {
    recordings: &'r Recordings,
    seen: HashSet<u64>,
    steps: Vec<Step>,
}

impl<'r> Walk<'r> {
    /// A walk that has reached nothing, answered from `recordings`.
    pub fn over(recordings: &'r Recordings) -> Self {
        Walk {
            recordings,
            seen: HashSet::new(),
            steps: Vec::new(),
        }
    }

    /// Every state reachable from the application as it opens, walked breadth first, each
    /// held by `checks` as it is reached and as an input is handed to it; how many states
    /// it reached. The frontier holds one ring of the walk at a time.
    pub fn walked(mut self, checks: &mut impl Checks) -> usize {
        let (opened, requests) = State::opened(now());
        self.recordings.carried_out(&requests, &self.opening());
        let first = self
            .reached(&opened, None, "opened".to_string(), checks)
            .expect("the application as it opens is the first state reached");
        let mut frontier = VecDeque::from([(first, opened)]);
        while let Some((at, state)) = frontier.pop_front() {
            for input in self.inputs(&state) {
                let label = format!("{input:?}");
                let mut next = state.clone();
                let asked = catch_unwind(AssertUnwindSafe(|| next.update(input.clone())))
                    .unwrap_or_else(|_| {
                        panic!(
                            "an input panicked, reached by:\n{}",
                            self.at(at, &label).inputs()
                        )
                    });
                let reached = self.at(at, &label);
                self.recordings.carried_out(&asked, &reached);
                checks.handed(&state, &input, &asked, &reached);
                if let Some(place) = self.reached(&next, Some(at), label, checks) {
                    frontier.push_back((place, next));
                }
            }
        }
        self.steps.len()
    }

    /// How the walk stands before the application opens.
    fn opening(&self) -> Reached<'_> {
        Reached {
            steps: &self.steps,
            last: None,
            then: vec!["opened".to_string()],
        }
    }

    /// How the walk comes to `input` handed to the state at `at`.
    fn at(&self, at: usize, input: &str) -> Reached<'_> {
        Reached {
            steps: &self.steps,
            last: Some(at),
            then: vec![input.to_string()],
        }
    }

    /// `state`, reached by `input` handed to the state at `from`, where it is new: its
    /// place among the steps, once `checks` hold it. Nothing where a state with the same
    /// screen waiting on the same kinds of work was reached first.
    fn reached(
        &mut self,
        state: &State,
        from: Option<usize>,
        input: String,
        checks: &mut impl Checks,
    ) -> Option<usize> {
        let screen = state.screen(&Markdown.look());
        let markdown = Markdown.screen(&screen);
        // What it waits on, by kind: the walk answers every piece of work of one kind
        // with the same recorded outcomes, whichever run it names.
        let kinds: BTreeSet<u64> = state
            .waiting_on()
            .iter()
            .map(|(_, work)| {
                let mut kind = DefaultHasher::new();
                work.kind().hash(&mut kind);
                kind.finish()
            })
            .collect();
        // Where each region is scrolled to, which Markdown, drawing every line, does not say.
        let tops = (
            screen.main.sight.top,
            screen.side.as_ref().map(|side| side.region.sight.top),
        );
        let mut fingerprint = DefaultHasher::new();
        (&markdown, kinds, tops).hash(&mut fingerprint);
        if !self.seen.insert(fingerprint.finish()) {
            return None;
        }
        self.steps.push(Step { from, input });
        let place = self.steps.len() - 1;
        let reached = Reached {
            steps: &self.steps,
            last: Some(place),
            then: Vec::new(),
        };
        assert!(
            self.seen.len() <= MOST_STATES,
            "the walk does not end: past {MOST_STATES} states, the last reached by:\n{}",
            reached.inputs()
        );
        checks.reached(state, &markdown, &reached);
        Some(place)
    }

    /// Every input `state` can be handed: each stroke while it waits on few enough pieces
    /// of work ([`STROKED_WHILE_WAITING`]), what the walk's terminal laid out of its
    /// screen, each page written that a surface could not show, and each recorded piece
    /// of news and answer of each piece of work it waits on. With a field open nothing is typed, and a list is narrowed by one character at
    /// most: what a typed line or a list's filter makes of more is its own tests', and
    /// words typed on would never end.
    fn inputs(&self, state: &State) -> Vec<Input> {
        let waiting = state.waiting_on();
        let typing = state.typing() || filtered(state);
        let stroked = waiting.len() <= STROKED_WHILE_WAITING;
        let strokes = strokes()
            .into_iter()
            .filter(|stroke| !(typing && stroke.types()))
            .filter(|_| stroked)
            .map(Input::Stroke);
        let answers = waiting.into_iter().flat_map(|(ticket, operation)| {
            let news = self
                .recordings
                .news_of(&operation)
                .iter()
                .map(move |news| Input::News(ticket, news.clone()));
            let outcomes = self
                .recordings
                .of(&operation)
                .iter()
                .map(move |outcome| Input::Outcome(ticket, outcome.clone()));
            news.chain(outcomes).collect::<Vec<_>>()
        });
        let unshown = self
            .recordings
            .pages_written()
            .into_iter()
            .map(Input::Unshown);
        strokes
            .chain([Input::Laid(laid_out(state))])
            .chain(unshown)
            .chain(answers)
            .collect()
    }
}

/// Every stroke but typing, and typing each key of [`KEYS`].
fn strokes() -> Vec<Stroke> {
    let mut strokes = vec![
        Stroke::Enter,
        Stroke::PastedBreak,
        Stroke::Backspace,
        Stroke::Delete,
        Stroke::Left,
        Stroke::Right,
        Stroke::Home,
        Stroke::End,
        Stroke::PageUp,
        Stroke::PageDown,
        Stroke::Decline,
        Stroke::Interrupt,
        Stroke::Up,
        Stroke::Down,
        Stroke::Tab,
        Stroke::BackTab,
        Stroke::Ignored,
    ];
    strokes.extend(KEYS.map(Stroke::Insert));
    strokes
}

/// Whether `state`'s screen shows a list narrowed by what was typed.
fn filtered(state: &State) -> bool {
    let screen = state.screen(&Markdown.look());
    screen
        .main
        .doc
        .blocks()
        .iter()
        .any(|block| matches!(block, Block::ChoiceList(list) if list.filter.is_some()))
}
