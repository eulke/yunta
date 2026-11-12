//! The walk: from the application as it opens, and as it opens on each step the
//! recordings answer the opening of ([`Recordings::opened_on`]), every stroke, what a
//! terminal of a few rows laid out of the screen, each page a surface could not show, and
//! every recorded piece of news and answer of each piece of work a state waits on, applied
//! to every state it reaches. Each state is walked once, kept by its screen, the kinds of
//! work it waits on, the rows its regions are scrolled to and the stages it has yet to
//! open. Its clock stands still at one instant: what time changes on a screen is each
//! stage's own tests'.
//!
//! The walk holds only what every walk needs: that it ends, that no input panics, and
//! that every piece of work a state asks for is one the recordings answer. What a surface
//! holds of every state it reaches is a [`Checks`] of its own: the interface's rules, a
//! terminal's layout. A state that breaks one is named with the inputs that reach it
//! ([`Reached`]).

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::num::NonZeroUsize;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

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

/// The states the walk expands at once for each of the machine's cores: enough that every
/// core has work while the slowest finishes its share, few enough that what they come to
/// is held for a moment, not for a whole ring of the walk.
const EXPANDED_PER_CORE: usize = 8;

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

/// How the walk tells a state apart: a fingerprint of its screen as Markdown draws it, the
/// kinds of work it waits on, the rows its regions are scrolled to and the stages it has
/// yet to open; and that Markdown, which the checks read.
struct Seen {
    fingerprint: u64,
    markdown: String,
}

impl Seen {
    /// How the walk tells `state` apart.
    fn of(state: &State) -> Seen {
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
        // The stages a step opened the application on, which no screen shows before it
        // first stands somewhere: each opening's own.
        let to_open = format!("{:?}", state.stages_to_open());
        let mut fingerprint = DefaultHasher::new();
        (&markdown, kinds, tops, to_open).hash(&mut fingerprint);
        Seen {
            fingerprint: fingerprint.finish(),
            markdown,
        }
    }
}

/// An input handed to a state on a core of its own: the input, as the walk names it, and
/// what the state came to; nothing where it panicked.
struct Handed {
    input: Input,
    label: String,
    came: Option<Came>,
}

/// What a state came to on an input: what it asked, how it is seen, and the state itself
/// where no state reached before was seen the same, since the walk keeps no other.
struct Came {
    asked: Vec<Request>,
    seen: Seen,
    next: Option<State>,
}

/// The walk over what `recordings` answer. Not the states it reaches: it keeps a
/// fingerprint of each and the step that first reached it.
pub struct Walk<'r> {
    recordings: &'r Recordings,
    fingerprints: HashSet<u64>,
    steps: Vec<Step>,
}

impl<'r> Walk<'r> {
    /// A walk that has reached nothing, answered from `recordings`.
    pub fn over(recordings: &'r Recordings) -> Self {
        Walk {
            recordings,
            fingerprints: HashSet::new(),
            steps: Vec::new(),
        }
    }

    /// Every state reachable from the application as it opens, then from it opened on
    /// each step the recordings name, each state held by `checks` as it is reached and as
    /// an input is handed to it; how many states it reached. Each opening is walked whole
    /// before the next: a state is kept by what shows, so walking them side by side would
    /// let an opening on a step reach first a screen the opening reaches, with other
    /// stages under it, and leave unwalked what only the opening's way on from it reaches.
    pub fn walked(mut self, checks: &mut impl Checks) -> usize {
        let opened_on = self.recordings.opened_on().iter().map(|step| {
            let opened = State::opened_on(now(), step).expect("a stage takes the step");
            (format!("opened on {step:?}"), opened)
        });
        let openings: Vec<_> = [("opened".to_string(), State::opened(now()))]
            .into_iter()
            .chain(opened_on)
            .collect();
        for (opening, (opened, requests)) in openings {
            self.recordings
                .carried_out(&requests, &self.opening(&opening));
            let seen = Seen::of(&opened);
            if let Some(first) = self.kept(&opened, seen, None, opening, checks) {
                self.walked_from(first, opened, checks);
            }
        }
        self.steps.len()
    }

    /// Every state reachable from `opened`, reached at `first`, walked breadth first. The
    /// states at the frontier's head are expanded together on every core: each input
    /// handed to a copy and the state it comes to drawn as Markdown draws it, which is most
    /// of a walk's work. What they came to is then taken in the order a walk of one input
    /// at a time takes it, so the same states are reached at the same steps, and every
    /// check runs on this thread.
    fn walked_from(&mut self, first: usize, opened: State, checks: &mut impl Checks) {
        let cores = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
        let mut frontier = VecDeque::from([(first, opened)]);
        while !frontier.is_empty() {
            let taken = frontier.len().min(cores * EXPANDED_PER_CORE);
            let expanding: Vec<(usize, State)> = frontier.drain(..taken).collect();
            let handed = self.handed_on_every_core(&expanding, cores);
            for ((at, state), handed) in expanding.iter().zip(handed) {
                for Handed { input, label, came } in handed {
                    let reached = self.at(*at, &label);
                    let Some(Came { asked, seen, next }) = came else {
                        panic!("an input panicked, reached by:\n{}", reached.inputs())
                    };
                    self.recordings.carried_out(&asked, &reached);
                    checks.handed(state, &input, &asked, &reached);
                    let kept = next.and_then(|next| {
                        let place = self.kept(&next, seen, Some(*at), label, checks)?;
                        Some((place, next))
                    });
                    frontier.extend(kept);
                }
            }
        }
    }

    /// Each input each of `expanding` can be handed, handed to it on `cores` threads: what
    /// each came to, in the order of `expanding` and of each one's inputs.
    fn handed_on_every_core(&self, expanding: &[(usize, State)], cores: usize) -> Vec<Vec<Handed>> {
        let share = expanding.len().div_ceil(cores).max(1);
        std::thread::scope(|scope| {
            let workers: Vec<_> = expanding
                .chunks(share)
                .map(|part| {
                    scope.spawn(move || {
                        part.iter()
                            .map(|(_, state)| self.handed_to(state))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|worker| worker.join().unwrap_or_else(|panic| resume_unwind(panic)))
                .collect()
        })
    }

    /// Each input `state` can be handed, handed to a copy of it: what the copy came to and
    /// asked, and how it is seen; the copy itself only where no state reached so far is
    /// seen the same.
    fn handed_to(&self, state: &State) -> Vec<Handed> {
        self.inputs(state)
            .into_iter()
            .map(|input| {
                let label = format!("{input:?}");
                let mut next = state.clone();
                let came = catch_unwind(AssertUnwindSafe(|| next.update(input.clone())))
                    .ok()
                    .map(|asked| {
                        let seen = Seen::of(&next);
                        let next = (!self.fingerprints.contains(&seen.fingerprint)).then_some(next);
                        Came { asked, seen, next }
                    });
                Handed { input, label, came }
            })
            .collect()
    }

    /// How the walk stands before the application opens as `opening` says.
    fn opening(&self, opening: &str) -> Reached<'_> {
        Reached {
            steps: &self.steps,
            last: None,
            then: vec![opening.to_string()],
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

    /// `state`, seen as `seen`, reached by `input` handed to the state at `from`, where it
    /// is new: its place among the steps, once `checks` hold it. Nothing where a state seen
    /// the same was reached first.
    fn kept(
        &mut self,
        state: &State,
        seen: Seen,
        from: Option<usize>,
        input: String,
        checks: &mut impl Checks,
    ) -> Option<usize> {
        if !self.fingerprints.insert(seen.fingerprint) {
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
            self.fingerprints.len() <= MOST_STATES,
            "the walk does not end: past {MOST_STATES} states, the last reached by:\n{}",
            reached.inputs()
        );
        checks.reached(state, &seen.markdown, &reached);
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
