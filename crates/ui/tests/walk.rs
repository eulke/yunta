//! The walk, as the interface holds it: over every state `yunta_testkit_ui`'s walk
//! reaches from the application as it opens, and beside them a remark of each piece of
//! work under way, which asks nothing. What every state holds: an input never panics,
//! every piece of work it asks for is one the performer carries out, no work that stops,
//! closes, removes or spends is asked but on its confirmation, its screen draws on
//! Markdown and HTML, and where its trail names a run, under any stage opened over the
//! run's, it follows that run. Over each state the palette is opened and every row of
//! each palette the walk meets is taken, and help is opened over each legend it meets,
//! holding the same. A state that breaks one is named with the inputs that reach it.
//! Among the inputs is what a terminal of a few rows laid out of the state's screen, so
//! every page a stage shows scrolls.

#[path = "walk/dialogs.rs"]
mod dialogs;

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use surco::components::{Screen, Sight};
use surco::surface::{Html, Markdown};
use yunta_core::RunId;
use yunta_testkit_ui::home::Home;
use yunta_testkit_ui::recorded::Recordings;
use yunta_testkit_ui::walk::{Checks, Reached, Walk};
use yunta_ui::{Input, Request, State};

use dialogs::{nothing_unconfirmed, MUST_OFFER};

/// What a screen the walk must reach shows, and what that screen is.
const MUST_REACH: [(&str, &str); 48] = [
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
    (
        "`↑↓` scroll · `esc` leaves the run waiting",
        "a plan opened on review",
    ),
    (
        "› REV1EW\n",
        "the run reached back from a plan opened on review",
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
/// scroll: a run's page stands as its handle, which every run of the walk's home ends its
/// id with.
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
/// recordings its work is answered from, the runs of the walk's home a trail names by
/// their handles, each palette taken row by row by its rows, and each help opened by the
/// keys it lists.
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
    /// Checks that have met nothing, with every screen of [`MUST_REACH`] yet to show,
    /// over `runs`, the runs of the walk's home.
    fn new(recordings: &'r Recordings, runs: Vec<RunId>) -> Self {
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
}

impl Checks for InterfaceChecks<'_> {
    fn reached(&mut self, state: &State, markdown: &str, reached: &Reached<'_>) {
        let _ = Html.screen(&state.screen(&Html.look()));
        self.unreached
            .retain(|(shown, _)| !markdown.contains(shown));
        let screen = state.screen(&Markdown.look());
        if let Some(page) = scrolled(&screen) {
            self.unscrolled.retain(|(at, _)| *at != page);
        }
        follows_the_run_it_names(state, &screen.trail, &self.runs, reached);
        self.palette_taken(state, reached);
        self.help_taken(state, reached);
        remarks_hold(state, reached);
    }

    fn handed(&mut self, from: &State, input: &Input, asked: &[Request], reached: &Reached<'_>) {
        nothing_unconfirmed(asked, from, input, reached);
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
        walked_whole(&recordings, runs_of(&home));
    });
}

/// The runs of the walk's home, which a trail names by their handles.
fn runs_of(home: &Home) -> Vec<RunId> {
    vec![
        home.worked.run_id.clone(),
        home.parked.clone(),
        home.deciding.run_id.clone(),
        home.asking.run_id.clone(),
        home.reviewing.run_id.clone(),
    ]
}

/// The walk over `recordings`, held to every screen it must reach, its trails naming
/// `runs`.
fn walked_whole(recordings: &Recordings, runs: Vec<RunId>) {
    let started = Instant::now();
    let mut checks = InterfaceChecks::new(recordings, runs);
    let states = Walk::over(recordings).walked(&mut checks);
    eprintln!(
        "the walk reached {states} states, took {} palettes row by row and opened {} helps, in {:?}",
        checks.palettes.len(),
        checks.helps.len(),
        started.elapsed()
    );
    assert!(
        checks.unreached.is_empty(),
        "the walk never reaches {:?}",
        checks
            .unreached
            .iter()
            .map(|(_, what)| what)
            .collect::<Vec<_>>()
    );
    assert!(
        checks.unoffered.is_empty(),
        "no palette the walk met offers {:?}",
        checks.unoffered
    );
    assert!(
        checks.unscrolled.is_empty(),
        "the walk never scrolls {:?}",
        checks
            .unscrolled
            .iter()
            .map(|(_, what)| what)
            .collect::<Vec<_>>()
    );
}

/// Where the page `screen` shows scrolled past its first line stands: the last place on
/// the trail; none where nothing was scrolled, or where the main region reads at its end.
/// A region beside the main one is a list that follows its pointer, which no key scrolls.
fn scrolled(screen: &Screen) -> Option<&str> {
    let main = screen.main.sight != Sight::at_the_end() && screen.main.sight.top > 0;
    main.then(|| screen.trail.last().map(String::as_str))
        .flatten()
}

/// Where `trail`, `state`'s, reached as `reached` says, names one of `runs` by its handle,
/// `state` follows a run of that handle: the run's stage follows its run under every stage
/// opened over it, its stats, its graph, its decision, its evidence, the palette and help,
/// so what each shows of the run is what the run comes to.
fn follows_the_run_it_names(
    state: &State,
    trail: &[String],
    runs: &[RunId],
    reached: &Reached<'_>,
) {
    let named = trail
        .iter()
        .find(|place| runs.iter().any(|run| run.handle() == place.as_str()));
    let Some(handle) = named else {
        return;
    };
    let followed = state.followed().run;
    assert!(
        followed.as_ref().is_some_and(|run| run.handle() == handle),
        "the trail names the run {handle} and the state follows {followed:?}, reached by:\n{}",
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
