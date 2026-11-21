//! See [`super`]. The walk, as the interface holds it, on every surface: over every state the
//! walk reaches, and beside them a remark of each piece of work under way, which asks nothing.
//! What every state holds: an input never panics, every piece of work it asks for is one the
//! performer carries out, no work that stops, closes, removes or spends is asked but on its
//! confirmation, its screen draws on Markdown and HTML, and where its trail names a run,
//! under any stage opened over the run's, it follows the run it names last. Over each state
//! the palette is opened and every row of each palette the walk meets is taken, and help is
//! opened over each legend it meets, holding the same (`dialogs`). Once the walk ends it has
//! shown every screen `MUST_REACH` names, scrolled every page `MUST_SCROLL` names and met a
//! palette offering every command `MUST_OFFER` names. A state that breaks one is named with
//! the inputs that reach it. Among the inputs is what a terminal of a few rows laid out of the
//! state's screen, so every page a stage shows scrolls.
//!
//! The rules stand here, beside the walk, and not in a test of `yunta-ui`: the test that
//! walks holds them beside a terminal's layout, so the states are reached once for both.

mod dialogs;

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

use surco::components::{Screen, Sight};
use surco::surface::{Html, Markdown};
use yunta_core::RunId;
use yunta_ui::{Input, ItsRun, Request, State};

use super::{Checks, Reached};
use crate::recorded::Recordings;
use dialogs::{nothing_unconfirmed, MUST_OFFER};

/// What a screen the walk must reach shows, and what that screen is: every screen of the
/// application, and every scene of a run's screen, as the log of a run of the walk's home
/// stands at it.
const MUST_REACH: [(&str, &str); 80] = [
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
    ("`j k` node", "a run"),
    ("Go on?", "a decision"),
    (
        "`↑↓` scroll · `esc` leave it waiting",
        "the plan's desk review opens on the run's screen",
    ),
    (
        "› REV1EW\n",
        "the run's screen review opens, before its run is read",
    ),
    (
        "`p` proof · `esc` go back",
        "a plan read from its card in place",
    ),
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
    ("nothing here could open it", "a page not shown"),
    ("000000 stats", "a run's stats"),
    ("● done · the nodes it runs, in order", "a run's graph"),
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
    // A run's scenes, each as the log says it.
    (
        "◌ tasks  not yet",
        "a session at work, what it hands over not yet handed",
    ),
    (
        "`test -f fixed.txt`\n\n### what it is doing · 0 calls\n\n**running: attempt 1**",
        "a command at work",
    ),
    (
        "● **global-pack-install**  Add explicit global pack install target",
        "a loop at work",
    ),
    ("## findings · 0 of 4 answered", "the findings"),
    (
        "- ◆ **Which release train carries it?**\n  - ▸ **this week**",
        "a round of choices",
    ),
    (
        "answers were refused: answers.yaml",
        "a round whose answers were refused",
    ),
    ("◆ **Publish them?**", "a gate's card, with no plan"),
    ("◆ **`tests` failed. What now?**", "a failure's card"),
    (
        "◆ **It asks to touch crates/cli/src/commands/pack_audit.rs**",
        "a scope's card",
    ),
    ("### the session says instead", "a departure's card"),
    (
        "◆ **run `T0KENS` exhausted its token budget**",
        "a token cap's card",
    ),
    (
        "needs another iteration and its cap is spent",
        "a loop cap's card",
    ),
    (
        "the run goes on when an engine takes it up",
        "an answer recorded, its resume not yet on the log",
    ),
    (
        "## implement · loop · executor on mock/gpt-6-luna\n\n▲ **global-pack-install**",
        "a pause on the log, what it asks not yet read",
    ),
    (
        "the document does not parse at `specs`",
        "a decision whose document cannot be read",
    ),
    (
        "no option answers it here: settle it where it was raised",
        "a pause whose escalation carries no menu",
    ),
    ("it goes on alone at", "a run that wakes alone"),
    (
        "waiting on its review",
        "a pull request read from its forge",
    ),
    (
        "a child of PARENT, opened by its node `feat`",
        "a child in its parent's place",
    ),
    ("needed  9000 TB", "a run with no room"),
    ("### left half done", "a run left half done"),
    ("interrupted by a person", "a run a person stopped"),
    (
        "no process is driving it: the engine that ran it is gone",
        "a stalled run",
    ),
    (
        "nothing here answers it: answer it there",
        "a question asked at another terminal",
    ),
    ("# ● two-steps (default) · created", "a run created"),
    ("pull request: #31", "the design's receipt"),
    ("## the findings that block", "a report's receipt"),
    ("fixed.txt is missing", "a failed run's end"),
    ("### its successor", "a promoted run's end"),
    ("### where its log stops making sense", "a broken log's end"),
    (
        "The mode `alpha` leaves it out.",
        "a node the mode leaves out",
    ),
];

/// What each page a stage shows is, by where it stands on the trail, which the walk must
/// scroll: a run's page stands as its handle, which the runs the walk's home made itself
/// end their ids with, and a page about a run or a workflow by its word after the handle or
/// the name. A run's numbers scroll on the design's run, which spent and found enough for
/// them to pass a screen.
const MUST_SCROLL: [(&str, &str); 9] = [
    ("000000", "a run's page, its nodes beside it"),
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

/// What the interface holds over the walk, and the screens it has yet to show: the
/// recordings its work is answered from, the runs of the walk's home, which a trail names
/// by their handles, each palette taken row by row by its rows, and each help opened
/// by the keys it lists. Not a terminal's layout, which the test that walks holds beside it.
pub struct InterfaceChecks<'r> {
    recordings: &'r Recordings,
    runs: Vec<RunId>,
    unreached: Vec<(&'static str, &'static str)>,
    unscrolled: Vec<(&'static str, &'static str)>,
    palettes: HashSet<u64>,
    unoffered: Vec<(&'static str, &'static str)>,
    helps: HashSet<u64>,
}

impl<'r> InterfaceChecks<'r> {
    /// Checks that have met nothing, with every screen the walk must reach yet to show,
    /// every page it must scroll yet to scroll and every command a palette must offer yet
    /// to be offered, over `runs`, the runs of the walk's home, its work answered from
    /// `recordings`.
    pub fn new(recordings: &'r Recordings, runs: Vec<RunId>) -> Self {
        InterfaceChecks {
            recordings,
            runs,
            unreached: MUST_REACH.to_vec(),
            unscrolled: MUST_SCROLL.to_vec(),
            palettes: HashSet::new(),
            unoffered: MUST_OFFER.to_vec(),
            helps: HashSet::new(),
        }
    }

    /// How many palettes the walk met and took row by row, each told apart by its rows.
    pub fn palettes_taken(&self) -> usize {
        self.palettes.len()
    }

    /// How many helps the walk opened, each over a legend told apart by its keys.
    pub fn helps_opened(&self) -> usize {
        self.helps.len()
    }

    /// Holds what the walk, once it ends, must have met: every screen it must reach shown,
    /// every command a palette must offer offered, and every page it must scroll scrolled;
    /// panics naming what it never met.
    pub fn met_all_it_must(&self) {
        assert!(
            self.unreached.is_empty(),
            "the walk never reaches {:?}",
            self.unreached
                .iter()
                .map(|(_, what)| what)
                .collect::<Vec<_>>()
        );
        assert!(
            self.unoffered.is_empty(),
            "no palette the walk met offers {:?}",
            self.unoffered
        );
        assert!(
            self.unscrolled.is_empty(),
            "the walk never scrolls {:?}",
            self.unscrolled
                .iter()
                .map(|(_, what)| what)
                .collect::<Vec<_>>()
        );
    }
}

impl Checks for InterfaceChecks<'_> {
    fn reached(&mut self, state: &State, markdown: &str, reached: &Reached<'_>) {
        let html = catch_unwind(AssertUnwindSafe(|| {
            Html.screen(&state.screen(&Html.look()))
        }));
        if html.is_err() {
            panic!(
                "its screen panicked as HTML draws it, reached by:\n{}",
                reached.inputs()
            );
        }
        self.unreached
            .retain(|(shown, _)| !markdown.contains(shown));
        let screen = state.screen(&Markdown.look());
        if let Some(page) = scrolled(&screen, state) {
            self.unscrolled.retain(|(at, _)| *at != page);
        }
        follows_the_run_it_names(state, &screen.trail, &self.runs, reached);
        self.palette_taken(state, reached);
        self.help_taken(state, &screen, reached);
        remarks_hold(state, reached);
    }

    fn handed(&mut self, from: &State, input: &Input, asked: &[Request], reached: &Reached<'_>) {
        nothing_unconfirmed(asked, from, input, reached);
    }
}

/// Where the page `screen`, `state`'s, shows scrolled past its first line stands: the last
/// place on the trail, by its last word where a page about a run or a workflow names it
/// before its own, or the run's handle before it where that place is a node of the run in
/// view, which a run's screen names after the run; none where nothing was scrolled, or where
/// the main region reads at its end. A region beside the main one is a list that follows its
/// pointer, which no key scrolls.
fn scrolled<'s>(screen: &'s Screen, state: &State) -> Option<&'s str> {
    if screen.main.sight == Sight::at_the_end() || screen.main.sight.top == 0 {
        return None;
    }
    let in_view = state.followed_runs().in_view.as_ref();
    let of_the_run = |place: &String| {
        in_view.is_some_and(|in_view| {
            let mut nodes = in_view.run.manifest.workflow.iter_nodes();
            nodes.any(|node| node.id.as_str() == place)
        })
    };
    match screen.trail.as_slice() {
        [.., run, node] if of_the_run(node) => Some(run.as_str()),
        [.., last] => last.rsplit(' ').next(),
        [] => None,
    }
}

/// Where `trail`, `state`'s, reached as `reached` says, names one of `runs` by its handle, a
/// place of its own or the first word of a page about the run (`Q78GRR stats`), the run it
/// names last is the one the stage the person is at stands to as it says
/// ([`ItsRun`]): a stage that shows a run as it moves, the run's screen, its numbers, its
/// whole chronicle, under the palette and help too, names the run the state follows, so
/// what it shows is what the run comes to, a child shown in its parent's place named after
/// the parent and the node that bore it, and followed; a stage that holds what was written
/// for a run names that run still where a successor took its place under it. The graph over
/// a run names its workflow, not the run.
fn follows_the_run_it_names(
    state: &State,
    trail: &[String],
    runs: &[RunId],
    reached: &Reached<'_>,
) {
    let named = trail.iter().rev().find_map(|place| {
        let first = place.split(' ').next()?;
        runs.iter()
            .any(|run| run.handle() == first)
            .then_some(first)
    });
    let Some(handle) = named else {
        return;
    };
    let (its, stands) = match state.its_run() {
        Some(ItsRun::WroteFor(run)) => (Some(run), "holds what was written for"),
        Some(ItsRun::Shows(_)) | None => (state.followed().run, "follows"),
    };
    assert!(
        its.as_ref().is_some_and(|run| run.handle() == handle),
        "the trail names the run {handle} and the stage {stands} {its:?}, reached by:\n{}",
        reached.inputs()
    );
}

/// A remark of each piece of work `state`, reached as `reached` says, waits on, handed to
/// a copy of it: it never panics and asks nothing, whichever stage stands. Not walked on:
/// a stage that keeps what work remarks keeps every remark, so the states it makes never
/// end.
fn remarks_hold(state: &State, reached: &Reached<'_>) {
    for (ticket, operation) in state.waiting_on() {
        let remarking = reached.then(format!("a remark of {operation:?}"));
        let mut remarked = state.clone();
        let said = REMARKED.to_string();
        let remark = Input::Remark(ticket, yunta_api::Remark::Unstopped { said });
        let asked = catch_unwind(AssertUnwindSafe(|| remarked.update(remark)))
            .unwrap_or_else(|_| panic!("a remark panicked, after:\n{}", remarking.inputs()));
        assert!(
            asked.is_empty(),
            "a remark asked {asked:?}, after:\n{}",
            remarking.inputs()
        );
    }
}
