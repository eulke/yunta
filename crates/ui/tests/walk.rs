//! The walk: from the application as it opens, every stroke and every piece of news and
//! answer each piece of work it waits on can have, applied to every state it reaches,
//! each state once by its screen and the work it waits on; and, beside them, a remark
//! of each piece of work under way, which asks nothing. Its clock stands still at one
//! instant: what time changes on a screen is each stage's own tests'. Among them is what a terminal
//! of a few rows laid out of the state's screen, which a surface hands back after it
//! draws, so every page a stage shows scrolls. What every state holds: an input
//! never panics, every piece of work it asks for is one the performer carries out, and
//! its screen draws on Markdown and HTML. Over each state the palette is opened, and
//! every row of each palette the walk meets is taken, holding the same. A state that
//! breaks one is named with the inputs that reach it.

#[path = "walk/dialogs.rs"]
mod dialogs;
#[path = "walk/home.rs"]
mod home;
#[path = "walk/recorded.rs"]
mod recorded;
mod support;
#[path = "walk/terminal.rs"]
mod terminal;

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use surco::doc::Block;
use surco::surface::{Html, Markdown};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Request, State};

use dialogs::{nothing_unconfirmed, MUST_OFFER};
use home::Home;
use recorded::Recordings;
use support::now;
use terminal::{laid_out, scrolled};

/// The most states the walk visits before it calls the application endless: more than
/// twice what its stages reach, counted in states rather than seconds so the bound holds
/// alike on a fast machine and on a slow runner beside the rest of the suite.
const MOST_STATES: usize = 40_000;

/// What a screen the walk must reach shows, and what that screen is.
const MUST_REACH: [(&str, &str); 45] = [
    ("not in a repository", "the signpost"),
    ("project name", "a setup"),
    ("runs on", "a runner's adapter asked"),
    ("leaves it to declare later", "a runner's model asked"),
    ("for this repo's CLAUDE.md", "a setup written"),
    ("no runs yet", "an empty field"),
    ("runs of this repository", "the field of this repository"),
    ("runs on this machine", "the field of this machine"),
    ("closed, newest first", "the field with its closed runs"),
    ("does the same at a shell", "a confirmation"),
    ("back to the runs", "a run"),
    ("Go on?", "a decision"),
    ("What changed?", "a round"),
    ("What do you want to build?", "the first run"),
    ("Which workflow builds it?", "a start"),
    ("checks it", "the catalog"),
    ("can run here", "a workflow checked"),
    ("What is the new workflow called?", "a new workflow named"),
    ("fresh.yaml", "a new workflow written"),
    ("1 case, 0 failed", "the cases run"),
    ("stopped before every case ran", "the cases stopped"),
    ("acme/demo", "the packs listed"),
    ("none installed", "no pack installed"),
    ("demo-says", "a pack audited"),
    (
        "remove acme/demo from this project?",
        "a pack's removal asked",
    ),
    ("What is the new pack called?", "a new pack named"),
    ("acme/fresh", "a new pack written"),
    ("Where is the pack?", "a pack's source asked"),
    ("ships executors it runs", "a pack's executors confirmed"),
    ("acme/tools · installed", "a pack installed"),
    ("receipt of done", "a run's receipt"),
    ("page.html", "a run's page for a browser"),
    ("every row", "a run's stats"),
    ("workflow done of run", "a run's graph"),
    ("cost per verified task over time", "a workflow's past runs"),
    ("no runs of workflow", "a workflow that never ran"),
    ("as written", "a workflow's graph"),
    ("yunta doctor --session", "doctor's findings"),
    ("It spends a prompt each", "a session per binding asked"),
    (
        "stopped before every session ended",
        "doctor's sessions stopped",
    ),
    ("would reclaim no run", "what collecting would reclaim"),
    ("removed for good", "a collection asked"),
    ("no run to reclaim", "a collection"),
    ("Which document's shape?", "the documents' shapes"),
    ("the keys a workflow reads", "a document's shape"),
];

/// What each page a stage shows is, by where it stands on the trail, which the walk must
/// scroll: a run's page beside its nodes stands as `beside`.
const MUST_SCROLL: [(&str, &str); 9] = [
    ("beside", "a run's page beside its nodes"),
    ("receipt", "a run's receipt"),
    ("stats", "a run's stats"),
    ("history", "a workflow's past runs"),
    ("graph", "a graph"),
    ("doctor", "doctor's findings"),
    ("upkeep", "what collecting would reclaim"),
    ("workflow", "a document's shape"),
    ("acme/demo", "a pack's audit"),
];

/// What every piece of work under way remarks, in the walk.
const REMARKED: &str = "the work under way remarks this as it works";

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
    unscrolled: Vec<(&'static str, &'static str)>,
    /// Each palette taken row by row, by its rows.
    palettes: HashSet<u64>,
    unoffered: Vec<(&'static str, &'static str)>,
    /// Each help opened, by the keys it lists.
    helps: HashSet<u64>,
}

impl Walked {
    /// A walk that has reached nothing, with every screen of [`MUST_REACH`] yet to show.
    fn new() -> Walked {
        Walked {
            seen: HashSet::new(),
            steps: Vec::new(),
            unreached: MUST_REACH.to_vec(),
            unscrolled: MUST_SCROLL.to_vec(),
            palettes: HashSet::new(),
            unoffered: MUST_OFFER.to_vec(),
            helps: HashSet::new(),
        }
    }

    /// `state`, reached by `step`, where it is new: its place among the steps, its
    /// screens drawn on Markdown and HTML, and what it shows checked off. Nothing
    /// where a state with the same screen waiting on the same kinds of work was reached
    /// first.
    fn reached(&mut self, state: &State, step: Step) -> Option<usize> {
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
        assert!(
            self.seen.len() <= MOST_STATES,
            "the walk does not end: past {MOST_STATES} states, the last reached by:\n{}",
            self.path_to(&step)
        );
        let _ = Html.screen(&state.screen(&Html.look()));
        self.unreached
            .retain(|(shown, _)| !markdown.contains(shown));
        if let Some(page) = scrolled(&screen) {
            self.unscrolled.retain(|(at, _)| *at != page);
        }
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

/// On a deep stack: the recordings run the repository's cases, each driving a run
/// whole inside the test, which a debug build's futures make deeper than a test
/// thread's stack.
#[test]
fn every_state_the_application_reaches_holds() {
    yunta_testkit::on_a_deep_stack(|| async {
        let home = Home::built().await;
        let recordings = Recordings::made(&home).await;
        walked_whole(&recordings);
    });
}

/// The walk over `recordings`, held to every screen it must reach.
fn walked_whole(recordings: &Recordings) {
    let started = Instant::now();
    let walked = walk(recordings);
    eprintln!(
        "the walk reached {} states, took {} palettes row by row and opened {} helps, in {:?}",
        walked.steps.len(),
        walked.palettes.len(),
        walked.helps.len(),
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
    assert!(
        walked.unoffered.is_empty(),
        "no palette the walk met offers {:?}",
        walked.unoffered
    );
    assert!(
        walked.unscrolled.is_empty(),
        "the walk never scrolls {:?}",
        walked
            .unscrolled
            .iter()
            .map(|(_, what)| what)
            .collect::<Vec<_>>()
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
        walked.palette_taken(&state, at, recordings);
        walked.help_taken(&state, at);
        remarks_hold(&state, at, &walked);
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
            nothing_unconfirmed(&asked, &state, &step.input, || walked.path_to(&step));
            if let Some(reached) = walked.reached(&next, step) {
                frontier.push_back((reached, next));
            }
        }
    }
    walked
}

/// Every input `state` can be handed: each stroke while it waits on few enough pieces
/// of work ([`STROKED_WHILE_WAITING`]), what the walk's terminal laid out of its screen,
/// and each recorded piece of news and answer of each piece of work it waits on. With a field open nothing is typed, and a list is
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
    strokes
        .chain([Input::Laid(laid_out(state))])
        .chain(answers)
        .collect()
}

/// A remark of each piece of work `state`, reached at `at`, waits on, handed to a copy of
/// it: it never panics and asks nothing, whichever stage stands. Not walked on: a stage
/// that keeps what work remarks keeps every remark, so the states it makes never end.
fn remarks_hold(state: &State, at: usize, walked: &Walked) {
    for (ticket, operation) in state.waiting_on() {
        let step = Step {
            from: Some(at),
            input: format!("a remark of {operation:?}"),
        };
        let mut remarked = state.clone();
        let said = REMARKED.to_string();
        let remark = Input::Remark(ticket, yunta_api::Remark::Unstopped { said });
        let asked = catch_unwind(AssertUnwindSafe(|| remarked.update(remark)))
            .unwrap_or_else(|_| panic!("a remark panicked, after:\n{}", walked.path_to(&step)));
        assert!(
            asked.is_empty(),
            "a remark asked {asked:?}, after:\n{}",
            walked.path_to(&step)
        );
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
