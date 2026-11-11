//! The walk: from the application as it opens, every stroke and every piece of news and
//! answer each piece of work it waits on can have, applied to every state it reaches,
//! each state once by its screen and the work it waits on. What every state holds: an input never panics,
//! every piece of work it asks for is one the performer carries out, and its screen
//! draws on Markdown and HTML. A state that breaks one is named with the inputs that
//! reach it.

#[path = "walk/home.rs"]
mod home;
#[path = "walk/recorded.rs"]
mod recorded;
mod support;

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem::discriminant;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

use surco::doc::Block;
use surco::surface::{Html, Markdown};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Request, State};

use home::Home;
use recorded::Recordings;
use support::now;

/// How long the whole walk may take: it is run with the rest of the suite.
const WITHIN: Duration = Duration::from_secs(60);

/// The most states the walk visits before it calls the application endless: about what
/// it walks in the minute it may take, far past the few thousand its stages reach.
const MOST_STATES: usize = 40_000;

/// What a screen the walk must reach shows, and what that screen is.
const MUST_REACH: [(&str, &str); 11] = [
    ("not in a repository", "the signpost"),
    ("no runs yet", "an empty field"),
    ("runs of this repository", "the field of this repository"),
    ("runs on this machine", "the field of this machine"),
    ("closed, newest first", "the field with its closed runs"),
    ("confirm", "a confirmation"),
    ("back to the runs", "a run"),
    ("Go on?", "a decision"),
    ("What changed?", "a round"),
    ("What do you want to build?", "the first run"),
    ("Which workflow builds it?", "a start"),
];

/// Each character a stage binds, and one no stage binds.
const KEYS: [char; 15] = [
    'q', 'j', 'k', ' ', 'r', 'c', 'x', 'a', 'd', 'f', 'w', 'p', 'n', '1', 'z',
];

/// The most pieces of work a state may wait on for the walk to try strokes on it. A
/// stroke does the same whatever is under way; what turns on the work under way is
/// asking more beside it, which a state waiting on one already meets. A state waiting
/// on more is answered until it waits on one.
const STROKED_WHILE_WAITING: usize = 1;

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

/// How the walk reached a state: the input, and the state it was handed to, by its
/// place among the steps; nothing before the application opens.
struct Step {
    from: Option<usize>,
    input: String,
}

/// What the walk keeps of where it has been: a fingerprint of each state by its screen
/// and the kinds of work it waits on, the step that first reached each, and the screens it has
/// yet to show.
struct Walked {
    seen: HashSet<u64>,
    steps: Vec<Step>,
    unreached: Vec<(&'static str, &'static str)>,
}

impl Walked {
    /// A walk that has reached nothing, with every screen of [`MUST_REACH`] yet to show.
    fn new() -> Walked {
        Walked {
            seen: HashSet::new(),
            steps: Vec::new(),
            unreached: MUST_REACH.to_vec(),
        }
    }

    /// `state`, reached by `step`, where it is new: its place among the steps, its
    /// screens drawn on Markdown and HTML, and what it shows checked off. Nothing
    /// where a state with the same screen waiting on the same kinds of work was reached
    /// first.
    fn reached(&mut self, state: &State, step: Step) -> Option<usize> {
        let markdown = Markdown.screen(&state.screen(&Markdown.look()));
        // What it waits on, by kind: the walk answers every piece of work of one kind
        // with the same recorded outcomes, whichever run it names.
        let kinds: BTreeSet<u64> = state
            .waiting_on()
            .iter()
            .map(|(_, work)| {
                let mut kind = DefaultHasher::new();
                discriminant(work).hash(&mut kind);
                kind.finish()
            })
            .collect();
        let mut fingerprint = DefaultHasher::new();
        (&markdown, kinds).hash(&mut fingerprint);
        if !self.seen.insert(fingerprint.finish()) {
            return None;
        }
        assert!(
            self.seen.len() <= MOST_STATES,
            "the walk does not end: past {MOST_STATES} states, the last reached by:\n{}",
            self.path_to(&step)
        );
        let _ = Html.screen(&state.screen(&Html.look()));
        self.unreached
            .retain(|(shown, _)| !markdown.contains(shown));
        self.steps.push(step);
        Some(self.steps.len() - 1)
    }

    /// The inputs `step` ends, first to last, one a line.
    fn path_to(&self, step: &Step) -> String {
        let mut inputs = vec![step.input.as_str()];
        let mut from = step.from;
        while let Some(at) = from {
            inputs.push(self.steps[at].input.as_str());
            from = self.steps[at].from;
        }
        inputs.reverse();
        inputs.join("\n")
    }
}

#[tokio::test]
async fn every_state_the_application_reaches_holds() {
    let home = Home::built().await;
    let recordings = Recordings::made(&home).await;
    let started = Instant::now();
    let walked = walk(&recordings);
    assert!(
        started.elapsed() < WITHIN,
        "the walk took {:?}",
        started.elapsed()
    );
    assert!(
        walked.unreached.is_empty(),
        "the walk never reaches {:?}",
        walked
            .unreached
            .iter()
            .map(|(_, what)| what)
            .collect::<Vec<_>>()
    );
    eprintln!(
        "the walk reached {} states in {:?}",
        walked.steps.len(),
        started.elapsed()
    );
}

/// Every state reachable from the application as it opens, answered from
/// `recordings`, walked breadth first, each state kept once it is new: the frontier
/// holds one ring of the walk at a time.
fn walk(recordings: &Recordings) -> Walked {
    let mut walked = Walked::new();
    let (opened, requests) = State::opened(now());
    let opening = Step {
        from: None,
        input: "opened".to_string(),
    };
    every_request_is_carried_out(&requests, recordings, || walked.path_to(&opening));
    let first = walked
        .reached(&opened, opening)
        .expect("the application as it opens is the first state reached");
    let mut frontier = VecDeque::from([(first, opened)]);
    while let Some((at, state)) = frontier.pop_front() {
        for input in inputs(&state, recordings) {
            let step = Step {
                from: Some(at),
                input: format!("{input:?}"),
            };
            let mut next = state.clone();
            let asked =
                catch_unwind(AssertUnwindSafe(|| next.update(input))).unwrap_or_else(|_| {
                    panic!("an input panicked, reached by:\n{}", walked.path_to(&step))
                });
            every_request_is_carried_out(&asked, recordings, || walked.path_to(&step));
            if let Some(reached) = walked.reached(&next, step) {
                frontier.push_back((reached, next));
            }
        }
    }
    walked
}

/// Every input `state` can be handed: each stroke while it waits on few enough pieces
/// of work ([`STROKED_WHILE_WAITING`]), and each recorded piece of news and answer of
/// each piece of work it waits on. With a field open nothing is typed, and a list is
/// narrowed by one character at most: what a typed line or a list's filter makes of
/// more is its own tests', and words typed on would never end.
fn inputs(state: &State, recordings: &Recordings) -> Vec<Input> {
    let waiting = state.waiting_on();
    let typing = state.typing() || filtered(state);
    let strokes = strokes()
        .into_iter()
        .filter(|stroke| !(typing && stroke.types()))
        .filter(|_| waiting.len() <= STROKED_WHILE_WAITING)
        .map(Input::Stroke);
    let answers = state
        .waiting_on()
        .into_iter()
        .flat_map(|(ticket, operation)| {
            let news = recordings
                .news_of(&operation)
                .iter()
                .map(move |news| Input::News(ticket, news.clone()));
            let outcomes = recordings
                .of(&operation)
                .iter()
                .map(move |outcome| Input::Outcome(ticket, outcome.clone()));
            news.chain(outcomes).collect::<Vec<_>>()
        });
    strokes.chain(answers).collect()
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

/// Every piece of work `requests` asks for is one the performer carried out for the
/// recordings; `path` says the inputs that asked where one is not.
fn every_request_is_carried_out(
    requests: &[Request],
    recordings: &Recordings,
    path: impl Fn() -> String,
) {
    for request in requests {
        if let Request::Perform(_, operation) = request {
            assert!(
                !recordings.of(operation).is_empty(),
                "{operation:?} has no recording in walk/recorded.rs; asked after:\n{}",
                path()
            );
        }
    }
}
