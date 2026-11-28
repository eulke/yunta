//! The walk: from the application as it opens, and as it opens on each step the
//! recordings answer the opening of ([`Recordings::opened_on`]), every stroke, what a terminal
//! of a few rows laid out of the screen, each page
//! a surface could not show, every recorded piece of news and answer of each piece of work a
//! state waits on, and every recorded piece of what it follows ([`Recordings::followed`]),
//! applied to every state it reaches. Each state is walked once, kept by its screen, the
//! kinds of work it waits on, the rows its regions are scrolled to, where a run's pane or the
//! page of its numbers stands and whether the last layout left it anything to scroll, the
//! stages it has yet to open and what it follows. Its clock stands still at one instant: what
//! time changes on a screen is each stage's own tests'.
//!
//! The palette and help are opened over each state too, and a box the walk has not met yet is
//! kept as a state of its own, which every check holds as it holds any: kept by the box, the
//! stage it stands over and the rows around it a box lays out among, and walked no further.
//! Once an opening is walked whole, each row of each palette it met is taken over the first
//! state it met that palette over, and a state a row leads to that no other way reached is
//! kept and held as any, and walked no further: what only a palette's row leads to, a run's
//! whole chronicle and the pages over a setup, the signpost or a run, is reached, its own way
//! on left to the stages' tests, which the walk's bound has no room for.
//!
//! What is followed is recorded by place and handed to every state that follows it,
//! whatever the state waits on, as a following hands it under no ticket. The state's fold
//! takes an event once and a piece handed again changes nothing, so a state handed a piece
//! it holds is the state it was: the news are bounded, as the recorded answers are.
//!
//! The walk holds only what every walk needs: that it ends, that no input panics, and
//! that every piece of work a state asks for, and everything it asks to follow, is one
//! the recordings answer. What else every state it reaches holds is a [`Checks`]: the
//! interface's rules, the same on every surface, stand beside the walk ([`interface`]);
//! a terminal's layout is its test's, which hands one walk both, so every state is reached
//! once for all of them. Each holds the states on a thread of its own, in the order the
//! walk reached them, while the cores expand the states after them (`checking`). A state
//! that breaks one is named with the inputs that reach it ([`Reached`]). What it reached is
//! counted by class as it keeps each state ([`classes`]). How it tells a state apart is
//! `seen`'s, and the moves it makes on one, a palette's rows among them, `moves`'.

mod checking;
pub mod classes;
pub mod interface;
mod moves;
mod seen;
mod strokes;

use std::collections::{HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

use surco::doc::Block;
use surco::surface::Markdown;
use yunta_ui::{Address, Input, Request, State};

use crate::recorded::Recordings;
use crate::sandbox::now;
use crate::terminal::laid_out;
use checking::{HandedOn, Kept, Unchecked};
use classes::Walked;
use moves::{Move, OPENS_HELP, OPENS_THE_PALETTE};
use seen::{opens_a_box, Seen};
use strokes::strokes;

/// The most states the walk visits before it calls the application endless: near a third
/// past the some 97,600 its openings reach, the runs whose scenes it opens among them, each
/// told apart by what it follows and by the frame a run's screen draws of the field under
/// it, and counted in states rather than seconds so the bound holds alike on a fast machine
/// and on a slow runner beside the rest of the suite.
const MOST_STATES: usize = 130_000;

/// The states the walk expands at once for each of the machine's cores: enough that every
/// core has work while the slowest finishes its share, few enough that what they come to
/// is held for a moment, not for a whole ring of the walk.
const EXPANDED_PER_CORE: usize = 8;

/// The most pieces of work a state may wait on for the walk to try strokes on it. A
/// stroke does the same whatever is under way; what turns on the work under way is
/// asking more beside it, which a state waiting on one already meets. A state waiting
/// on more is answered until it waits on one.
const STROKED_WHILE_WAITING: usize = 1;

/// What a reader of the walk holds of what it reaches, on a thread of its own: the
/// interface's rules, or a surface's. Not the walk's own rules, which every walk holds.
pub trait Checks: Send {
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
    then: Vec<Then<'w>>,
}

/// A step past those the walk kept: an input handed to a state, named only when the inputs
/// are read, as most inputs come to a state already reached and are never named; or the
/// words a check or an opening names its step with.
#[derive(Clone)]
enum Then<'w> {
    Handed(&'w Input),
    Named(String),
}

impl Reached<'_> {
    /// The inputs, first to last, one a line.
    pub fn inputs(&self) -> String {
        let mut inputs: Vec<String> = self
            .then
            .iter()
            .rev()
            .map(|then| match then {
                Then::Handed(input) => format!("{input:?}"),
                Then::Named(named) => named.clone(),
            })
            .collect();
        let mut at = self.last;
        while let Some(step) = at {
            inputs.push(self.steps[step].input.clone());
            at = self.steps[step].from;
        }
        inputs.reverse();
        inputs.join("\n")
    }

    /// This, then `input`: how a check names a step it takes off the walk.
    pub fn then(&self, input: impl Into<String>) -> Reached<'_> {
        let mut then = self.then.clone();
        then.push(Then::Named(input.into()));
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

/// A move made on a state on a core of its own, and what the state came to; nothing where it
/// panicked.
struct Handed {
    moved: Move,
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
/// fingerprint of each, the step that first reached it and its class.
pub struct Walk<'r> {
    recordings: &'r Recordings,
    fingerprints: HashSet<u64>,
    /// Each palette whose rows the walk took, by its rows: a palette's rows are taken over the
    /// first state the walk meets it over.
    palettes: HashSet<u64>,
    steps: Vec<Step>,
    walked: Walked,
}

impl<'r> Walk<'r> {
    /// A walk that has reached nothing, answered from `recordings`.
    pub fn over(recordings: &'r Recordings) -> Self {
        Walk {
            recordings,
            fingerprints: HashSet::new(),
            palettes: HashSet::new(),
            steps: Vec::new(),
            walked: Walked::default(),
        }
    }

    /// Every state reachable from the application as it opens, then from it opened on
    /// each step the recordings name, each state held by every one of `checks` as it is
    /// reached and as an input is handed to it; what it reached, how many states and how many
    /// of them in each class. Each opening is walked whole before the next: a state is kept
    /// by what shows, so walking them side by side would let an opening on a step reach first
    /// a screen the opening reaches, with other stages under it, and leave unwalked what only
    /// the opening's way on from it reaches.
    pub fn walked(mut self, checks: &mut [&mut dyn Checks]) -> Walked {
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
            if let Some(first) = self.kept(seen.fingerprint, &opened, None, move || opening) {
                for checks in checks.iter_mut() {
                    checks.reached(&opened, &seen.markdown, &self.reached_at(first));
                }
                self.walked_from(first, opened, checks);
            }
        }
        self.walked.states = self.steps.len();
        self.walked
    }

    /// Every state reachable from `opened`, reached at `first`, walked breadth first. The
    /// states at the frontier's head are expanded together on every core: each input
    /// handed to a copy and the state it comes to drawn as Markdown draws it, which is most
    /// of a walk's work. What they came to is then taken in the order a walk of one input
    /// at a time takes it, so the same states are reached at the same steps. Each of
    /// `checks` then holds what they came to on a thread of its own, in that same order,
    /// while the cores expand the states after them: a check takes the walk no time of its
    /// own while the cores have work.
    fn walked_from(&mut self, first: usize, opened: State, checks: &mut [&mut dyn Checks]) {
        let cores = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
        let mut frontier = VecDeque::from([(first, opened)]);
        let mut unchecked = Unchecked::default();
        let mut palettes_met = Vec::new();
        while !frontier.is_empty() {
            let taken = frontier.len().min(cores * EXPANDED_PER_CORE);
            let expanding: Vec<(usize, State)> = frontier.drain(..taken).collect();
            let last = std::mem::take(&mut unchecked);
            let handed =
                self.handed_on_every_core(&expanding, cores, || self.checked(&last, checks));
            for (to, ((at, state), handed)) in expanding.iter().zip(handed).enumerate() {
                let handed = self.rows_kept(&mut palettes_met, (*at, state), handed);
                for Handed { moved, came } in handed {
                    let reached = self.at(*at, &moved);
                    let Some(Came { asked, seen, next }) = came else {
                        panic!("an input panicked, reached by:\n{}", reached.inputs())
                    };
                    self.recordings.carried_out(&asked, &reached);
                    let kept = next.and_then(|next| {
                        let named = || moved.named();
                        let place = self.kept(seen.fingerprint, &next, Some(*at), named)?;
                        if !opens_a_box(&next) {
                            frontier.push_back((place, next.clone()));
                        }
                        Some(Kept {
                            place,
                            state: next,
                            markdown: seen.markdown,
                        })
                    });
                    unchecked.handed.push(HandedOn {
                        to,
                        moved,
                        asked,
                        kept,
                    });
                }
            }
            unchecked.expanded = expanding;
        }
        self.checked(&unchecked, checks);
        self.palettes_taken(palettes_met, checks);
    }

    /// Each input each of `expanding` can be handed, handed to it on `cores` threads, and
    /// `meanwhile` done on this one: what each came to, in the order of `expanding` and of
    /// each one's inputs.
    fn handed_on_every_core(
        &self,
        expanding: &[(usize, State)],
        cores: usize,
        meanwhile: impl FnOnce(),
    ) -> Vec<Vec<Handed>> {
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
            meanwhile();
            workers
                .into_iter()
                .flat_map(|worker| worker.join().unwrap_or_else(|panic| resume_unwind(panic)))
                .collect()
        })
    }

    /// Each move the walk makes on `state`, made on a copy of it: what the copy came to and
    /// asked, and how it is seen; the copy itself only where no state reached so far is
    /// seen the same.
    fn handed_to(&self, state: &State) -> Vec<Handed> {
        self.moves(state)
            .into_iter()
            .map(|moved| {
                let mut next = state.clone();
                let came = catch_unwind(AssertUnwindSafe(|| moved.made(&mut next)))
                    .ok()
                    .map(|asked| {
                        let seen = Seen::of(&next);
                        let next = (!self.fingerprints.contains(&seen.fingerprint)).then_some(next);
                        Came { asked, seen, next }
                    });
                Handed { moved, came }
            })
            .collect()
    }

    /// How the walk stands before the application opens as `opening` says.
    fn opening(&self, opening: &str) -> Reached<'_> {
        Reached {
            steps: &self.steps,
            last: None,
            then: vec![Then::Named(opening.to_string())],
        }
    }

    /// How the walk comes to `moved` made on the state at `at`.
    fn at<'a>(&'a self, at: usize, moved: &'a Move) -> Reached<'a> {
        let then = match moved {
            Move::Input(input) => Then::Handed(input),
            Move::PaletteRow { .. } => Then::Named(moved.named()),
        };
        Reached {
            steps: &self.steps,
            last: Some(at),
            then: vec![then],
        }
    }

    /// How the walk came to the state it kept at `place`.
    fn reached_at(&self, place: usize) -> Reached<'_> {
        Reached {
            steps: &self.steps,
            last: Some(place),
            then: Vec::new(),
        }
    }

    /// `state`, told apart by `fingerprint`, reached by the input `named` names handed to the
    /// state at `from`, kept where it is new and counted in its class: its place among the
    /// steps, the input named then and only then. Nothing where a state seen the same was
    /// reached first.
    fn kept(
        &mut self,
        fingerprint: u64,
        state: &State,
        from: Option<usize>,
        named: impl FnOnce() -> String,
    ) -> Option<usize> {
        if !self.fingerprints.insert(fingerprint) {
            return None;
        }
        self.walked.kept(state);
        let input = named();
        self.steps.push(Step { from, input });
        let place = self.steps.len() - 1;
        assert!(
            self.fingerprints.len() <= MOST_STATES,
            "the walk does not end: past {MOST_STATES} states, the last reached by:\n{}",
            self.reached_at(place).inputs()
        );
        Some(place)
    }

    /// Every input `state` can be handed: each stroke while it waits on few enough pieces
    /// of work ([`STROKED_WHILE_WAITING`]), what the walk's terminal laid out of its
    /// screen, each page written that a surface could not show, each recorded piece of
    /// news and answer of each piece of work it waits on, and each recorded piece of what
    /// it follows. With a field open nothing is typed, and a list is narrowed by one
    /// character at most: what a typed line or a list's filter makes of more is its own
    /// tests', and words typed on would never end.
    fn inputs(&self, state: &State) -> Vec<Input> {
        let waiting = state.waiting_on();
        let typing = state.typing() || filtered(state);
        let stroked = waiting.len() <= STROKED_WHILE_WAITING;
        let strokes = strokes()
            .into_iter()
            .chain([OPENS_THE_PALETTE, OPENS_HELP])
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
            .map(|page| Input::Unshown(Address::Page(page)));
        let followed = self.recordings.followed(state);
        let news = followed
            .into_iter()
            .map(|news| Input::FollowNews(news.clone()));
        strokes
            .chain([Input::Laid(laid_out(state))])
            .chain(unshown)
            .chain(answers)
            .chain(news)
            .collect()
    }
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
