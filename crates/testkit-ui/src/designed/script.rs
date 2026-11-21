//! What a recording does, in order: each wake of the run, what the recording
//! waits to see on the log before it moves the clock and gives the next word,
//! and each answer a person gives between two wakes ([`Step`], [`Act`]).
//! [`played`] plays a timeline; a small run plays its own, and does what no
//! step names (a wake a person answers at, an event written by hand) beside it.
//!
//! The design's timeline is [`TIMELINE`]. Its seconds are the design's: each
//! node works as long as the design shows it working, and each pause waits as
//! long as the design shows a person taking.
//!
//! A node at work beside others gives its checkout back after its close is on
//! the log, and the pool dates that give-back by the run's clock, which ranks the
//! checkout for whoever takes one next. So the clock moves past such a node only
//! once its checkout is given back ([`Act::GivenBack`]): moved first, it would
//! date the give-back by the move or not, as the engine and the recording race.

use yunta_core::events::StoredEvent;
use yunta_testkit::wait_until_async;

use super::place::Place;

/// What the recording does while a wake drives the run.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Act {
    /// Waits until the log holds this many events of a kind, naming a node or
    /// none: the event the last word let go, after which the run waits on a
    /// word again.
    Logged(&'static str, Option<&'static str>, usize),
    /// Waits until the command of this name is held, waiting for its word.
    Held(&'static str),
    /// Moves the run's clock on by this many seconds.
    Moves(u64),
    /// Gives the word of this name.
    Gives(&'static str),
    /// Makes the next run of the command of this name hold for its word.
    Arms(&'static str),
    /// Makes the next run of the suite fail.
    FailsOnce,
    /// Holds the start of the unit of this name (`node/<id>`, `task/<id>`),
    /// before it writes anything, at the first call made through its index.
    HoldsStart(&'static str),
    /// Waits until this many surveys of the pool are held, the recording
    /// having armed `surveys`, then lets the survey made first go on, and no
    /// survey made after it hold: of units started together, the one started
    /// first takes its checkout first.
    LetsSurvey(usize),
    /// Waits until the unit of this name is held at its start, and lets it
    /// start.
    LetsStart(&'static str),
    /// Waits until checkout `slot-N` of the run's pool is given back, by its
    /// number: what a node at work beside others worked in, its close on the
    /// log already.
    GivenBack(u32),
}

/// What the recording does with the run.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Step {
    /// Moves the run's clock on by this many seconds while the run is parked.
    Moves(u64),
    /// Makes the next run of the command of this name hold for its word.
    Arms(&'static str),
    /// Answers the round a node asked, each question by its id.
    Round(&'static str, &'static [(&'static str, &'static str)]),
    /// Answers the decision the run is parked on with this option.
    Decides(&'static str),
    /// A person writes a file of the run's tree, by its path, while the run is
    /// parked: what the run finds there on its next wake.
    Edits(&'static str, &'static str),
    /// Wakes the run, doing these acts as it goes, until it parks or finishes.
    Wakes(&'static [Act]),
}

use Act::{
    Arms, FailsOnce, GivenBack, Gives, Held, HoldsStart, LetsStart, LetsSurvey, Logged, Moves,
};

/// The kind of event a session's opening writes.
pub(crate) const OPENED: &str = "agent_session_opened";
/// The kind of event each step a session plays writes: a call, a note, a usage.
pub(crate) const SAID: &str = "agent_message";
/// The kind of event a node's close writes.
pub(crate) const FINISHED: &str = "node_finished";

/// The run measures its suite and lint beside `grill`, which asks its round.
const ASKING: &[Act] = &[
    Logged(OPENED, Some("grill"), 1),
    Held("suite"),
    Gives("suite"),
    Logged("baseline_captured", None, 1),
    Held("clippy"),
    Gives("clippy"),
    Logged("baseline_held", None, 1),
    Moves(2),
    Gives("grill-1"),
    Logged(SAID, Some("grill"), 3),
    Moves(14),
    Gives("grill-2"),
    Logged(SAID, Some("grill"), 6),
    Moves(20),
    Gives("grill-3"),
];

/// `brief`, `plan` and `spec`, up to the plan's gate.
const PLANNING: &[Act] = &[
    Logged(OPENED, Some("brief"), 1),
    Moves(10),
    Gives("brief-1"),
    Logged(SAID, Some("brief"), 4),
    Moves(12),
    Gives("brief-2"),
    Logged(OPENED, Some("plan"), 1),
    Moves(96),
    Gives("plan-1"),
    Logged(SAID, Some("plan"), 4),
    Moves(173),
    Gives("plan-2"),
    Logged(SAID, Some("plan"), 10),
    Moves(33),
    Gives("plan-3"),
    Logged(SAID, Some("plan"), 13),
    Moves(40),
    Gives("plan-4"),
    Logged(SAID, Some("plan"), 14),
    Moves(43),
    Gives("plan-5"),
    Logged(OPENED, Some("spec"), 1),
    Moves(250),
    Gives("spec-1"),
    Logged(SAID, Some("spec"), 4),
    Moves(500),
    Gives("spec-2"),
    Logged(SAID, Some("spec"), 9),
    Moves(83),
    Gives("spec-3"),
];

/// The task's first session, which asks for a path the fence refused it, and
/// the fresh session its pending request opens, up to the scope owed.
const BUILDING: &[Act] = &[
    Logged(OPENED, Some("implement"), 1),
    Logged("write_refused", Some("implement"), 1),
    Moves(600),
    Gives("implement-1"),
    Logged(SAID, Some("implement"), 5),
    Moves(621),
    Gives("implement-2"),
    Logged(SAID, Some("implement"), 10),
    Moves(370),
    Gives("implement-3"),
    Logged(SAID, Some("implement"), 15),
    Moves(9),
    Gives("implement-4"),
    Logged(OPENED, Some("implement"), 2),
    Moves(50),
    Gives("implement-5"),
    Logged(SAID, Some("implement"), 17),
    Moves(50),
    Gives("implement-6"),
];

/// The session picked back up with the path granted, which asks for another,
/// and the fresh session that request opens, up to the scope owed again.
const WIDENING: &[Act] = &[
    Logged(OPENED, Some("implement"), 3),
    Moves(200),
    Gives("implement-7"),
    Logged(SAID, Some("implement"), 24),
    Moves(9),
    Gives("implement-8"),
    Logged(OPENED, Some("implement"), 4),
    Moves(50),
    Gives("implement-9"),
    Logged(SAID, Some("implement"), 26),
    Moves(50),
    Gives("implement-10"),
];

/// The task done, the three reviewers at work at once, the findings answered,
/// the lint sent back to `fix-lint` and passed, and the suite failing.
const REVIEWING: &[Act] = &[
    Logged(OPENED, Some("implement"), 5),
    Moves(300),
    Gives("implement-11"),
    Logged(SAID, Some("implement"), 30),
    HoldsStart("node/review@reviewer"),
    HoldsStart("node/review@reviewer-alt"),
    HoldsStart("node/conform"),
    Moves(156),
    Gives("implement-12"),
    LetsStart("node/review@reviewer"),
    Logged(OPENED, Some("review@reviewer"), 1),
    LetsStart("node/review@reviewer-alt"),
    Logged(OPENED, Some("review@reviewer-alt"), 1),
    LetsStart("node/conform"),
    Logged(OPENED, Some("conform"), 1),
    Moves(10),
    Gives("review-1"),
    Logged(SAID, Some("review@reviewer"), 4),
    Moves(5),
    Gives("review-alt-1"),
    Logged(SAID, Some("review@reviewer-alt"), 3),
    Moves(5),
    Gives("conform-1"),
    Logged(SAID, Some("conform"), 2),
    Moves(5),
    Gives("conform-2"),
    Logged(FINISHED, Some("conform"), 1),
    GivenBack(3),
    Moves(17),
    Gives("review-alt-2"),
    Logged(FINISHED, Some("review@reviewer-alt"), 1),
    GivenBack(1),
    Moves(78),
    Gives("review-2"),
    Logged(OPENED, Some("fix-findings"), 1),
    Moves(130),
    Gives("fix-findings-1"),
    Logged(SAID, Some("fix-findings"), 4),
    Moves(200),
    Gives("fix-findings-2"),
    Logged(SAID, Some("fix-findings"), 11),
    Moves(1),
    Arms("clippy"),
    Gives("fix-findings-3"),
    Held("clippy"),
    Moves(6),
    Gives("clippy"),
    Logged(OPENED, Some("fix-lint"), 1),
    Moves(60),
    Gives("fix-lint-1"),
    Logged(SAID, Some("fix-lint"), 5),
    Moves(31),
    Arms("clippy"),
    Arms("suite"),
    FailsOnce,
    Gives("fix-lint-2"),
    Held("clippy"),
    Moves(5),
    Gives("clippy"),
    Held("suite"),
    Moves(32),
    Gives("suite"),
];

/// The suite run again, passing, up to the last gate.
const RETRYING: &[Act] = &[Held("suite"), Moves(32), Gives("suite")];

/// The pull request opened, and the run finished.
const SHIPPING: &[Act] = &[];

/// What the person who reads the suite's failure leaves in `commands/pack.rs`
/// before they ask for the suite again: the target each command is handed is
/// read once, where the commands are dispatched.
const READ_ONCE: &str = "//! `yunta pack`: add, update, remove and list, on the target chosen.\n\n\
                         pub async fn add(source: &str, confirmed_executors: bool, run_tests: bool, \
                         root: &Path, look: &Look) -> Result<Outcome, CliError> {\n    \
                         install(source, confirmed_executors, run_tests, root, look).await\n}\n\n\
                         pub fn remove(pack: &PackRef, root: &Path, look: &Look) -> \
                         Result<Outcome, CliError> {\n    uninstall(pack, root, look)\n}\n\n\
                         pub fn list(lock: &Path, look: &Look) -> Result<Outcome, CliError> {\n    \
                         listed(lock, look)\n}\n";

/// Everything the design's recording does, from the run created to its end.
pub(super) const TIMELINE: &[Step] = &[
    Step::Arms("suite"),
    Step::Arms("clippy"),
    Step::Wakes(ASKING),
    Step::Moves(300),
    Step::Round(
        "grill",
        &[
            (
                "where-global-lives",
                "The current user's platform data directory, shared by that user's projects",
            ),
            (
                "how-requested",
                "--global on pack add, update, remove and list",
            ),
        ],
    ),
    Step::Wakes(PLANNING),
    Step::Moves(605),
    Step::Decides("approve"),
    Step::Wakes(BUILDING),
    Step::Moves(600),
    Step::Decides("grant"),
    Step::Wakes(WIDENING),
    Step::Moves(826),
    Step::Decides("grant"),
    Step::Wakes(REVIEWING),
    Step::Moves(60),
    Step::Edits("crates/cli/src/commands/pack.rs", READ_ONCE),
    Step::Wakes(&[]),
    Step::Arms("suite"),
    Step::Decides("retry"),
    Step::Wakes(RETRYING),
    Step::Decides("approve"),
    Step::Wakes(SHIPPING),
];

/// How many of the log's last events the recording says when it waited in vain:
/// enough to read the step the run took instead.
const TAIL: usize = 8;

/// Plays `timeline` with the run `place` holds, created.
pub(crate) async fn played(place: &Place, timeline: &[Step]) {
    for step in timeline {
        match *step {
            Step::Moves(seconds) => place.moves(seconds),
            Step::Arms(name) => place.arm(name),
            Step::Round(node, answers) => {
                place
                    .bench
                    .answer_round(node, answers)
                    .await
                    .expect("the round takes its answers");
            }
            Step::Decides(option) => {
                place
                    .bench
                    .answer_parked(option, None)
                    .await
                    .expect("the decision takes its answer");
            }
            Step::Edits(path, content) => {
                tokio::fs::write(place.bench.tree().join(path), content)
                    .await
                    .expect("the person's edit of the run's tree");
            }
            Step::Wakes(acts) => {
                let (_report, ()) = tokio::join!(place.bench.wake(), acted(place, acts));
            }
        }
    }
}

/// Does `acts` while a wake drives the run.
pub(crate) async fn acted(place: &Place, acts: &[Act]) {
    for act in acts {
        match *act {
            Logged(kind, node, count) => {
                waited(
                    place,
                    || logged(&place.bench.events(), kind, node) >= count,
                    || format!("{count} `{kind}` of {node:?}"),
                )
                .await
            }
            Held(name) => waited(place, || place.holds(name), || format!("`{name}` held")).await,
            Moves(seconds) => place.moves(seconds),
            Gives(word) => place.give(word),
            Arms(name) => place.arm(name),
            FailsOnce => place.fails_once(),
            HoldsStart(unit) => place.arm(&Place::start_of(unit)),
            LetsSurvey(count) => {
                waited(
                    place,
                    || place.held_surveys().len() >= count,
                    || format!("{count} surveys of the pool held"),
                )
                .await;
                place.disarm("surveys");
                let first = place.held_surveys().first().copied();
                place.let_survey(first.expect("a survey held"));
            }
            GivenBack(number) => {
                waited(
                    place,
                    || place.given_back(number),
                    || format!("`slot-{number}` given back"),
                )
                .await
            }
            LetsStart(unit) => {
                let start = Place::start_of(unit);
                waited(
                    place,
                    || place.holds(&start),
                    || format!("`{unit}` held at its start"),
                )
                .await;
                place.give(&start);
            }
        }
    }
}

/// How many events of `kind` naming `node` the log holds.
fn logged(events: &[StoredEvent], kind: &str, node: Option<&str>) -> usize {
    events
        .iter()
        .filter(|event| {
            event.body.kind_name() == kind && event.node_id.as_ref().map(|id| id.as_str()) == node
        })
        .count()
}

/// Waits until `holds`; when it never does, says what it waited for and where
/// the run's log stands.
async fn waited(place: &Place, mut holds: impl FnMut() -> bool, what: impl FnOnce() -> String) {
    wait_until_async(
        || {
            let held = holds();
            async move { held }
        },
        || {
            let events = place.bench.events();
            let tail: Vec<String> = events
                .iter()
                .rev()
                .take(TAIL)
                .rev()
                .map(|event| serde_json::to_string(event).expect("an event as the log keeps it"))
                .collect();
            format!(
                "the recording waited for {} and the log ends:\n{}",
                what(),
                tail.join("\n")
            )
        },
    )
    .await;
}
