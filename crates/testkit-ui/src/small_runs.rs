//! The small runs: one for each scene of a run's screen the design's run does
//! not pass through, two past runs of `yunta/fragua` in a repository set up
//! as the design's ([`past::runs`]), and the two older runs every run's screen
//! stands beside in its field ([`beside`]). Each is built in the process that reads it,
//! with a fixed id, as the engine's own tests build that state, in a place
//! [`crate::designed`]'s recording makes and on a clock moved the way it moves
//! its own; built twice, it writes the same log ([`SmallRun::log`]). None is
//! kept.
//!
//! Each row of the scenes a run's screen shows, and what reaches it:
//!
//! - a session, a command or a check at work; findings; `Menu::Scope`; a pause
//!   on the log not yet read, and an answer recorded before its resume: the
//!   design's run, at its moments.
//! - a loop: the design's run, one task; [`work::two_tasks`], two at once.
//! - `Questions`: the design's run; `AnswersRefused`: [`asking::refused`].
//! - a gate that shows a plan: the design's run; sent back: [`asking::sent_back`].
//! - `Menu::Console`, a gate with no plan: [`asking::console_gate`]; a gate
//!   that shows a document that is no plan, its own `Menu::Gate`:
//!   [`asking::notes_gate`].
//! - `Menu::Failure`, `NodeFailed`: [`asking::failure`], and the design's suite.
//! - a departure: [`asking::departure`]; the loop cap: [`asking::loop_cap`].
//!   Parked, each rebuilds a `Menu::Failure`, the task stopped owing its
//!   departure and the loop capped: `Menu::Departure` and `Menu::LoopCap`
//!   are what a live terminal is asked, and no run is cut there.
//! - `Menu::Budget`: [`asking::token_cap`].
//! - a decision whose shown document cannot be read: [`asking::unread`].
//! - a pause whose escalation carries no menu: [`standstills::no_menu`].
//! - `AwaitingReset`: [`waiting::wakes_alone`].
//! - `ExternalGate`, `Menu::Published`: [`waiting::pull_request`].
//! - `ChildPaused`: [`waiting::child`].
//! - `NoRoom`: [`standstills::no_room`].
//! - `UncertainOrphans`: [`standstills::orphans`]; `Blocked`: [`standstills::blocked`].
//! - `Cancelled`: [`standstills::stopped`]; `GateAborted`: [`standstills::aborted`].
//! - stalled: [`standstills::stalled`], once a registry names a gone engine.
//! - a question open at another terminal: [`standstills::asked_elsewhere`], cut
//!   before its answer, once a registry names a live engine asking.
//! - created, no node started: [`ends::created`].
//! - finished: the design's run; reported: [`ends::reported`].
//! - failed: [`ends::failed`]; promoted: [`ends::promoted`]; broken:
//!   [`ends::broken`].
//! - a node pointed at while the run does not lead: any run; a node the mode
//!   leaves out, [`work::mode_skips`]; a node sent back, [`asking::sent_back`].
//! - the field every run's screen stands in, beside the run in view: one run
//!   that needs the person, [`beside::not_logged_in`], and one stalled,
//!   [`beside::long_stalled`], each older than the design's run.
//!
//! No run reaches `PullRequestClosed`, `CancelledAfterCrash` or `Recorded`,
//! each a scene a row above shares. What a registry says of a run's engine is not a
//! run's: a reader of a small run writes the registry its scene needs. What
//! no wake of this binary writes (a pause with no menu, a pause blocked, a
//! crash's start, a document accepted again, a close that names no node) is
//! appended as the engine writes it, as the engine's own tests append a crash.
//! A workspace that reads them reaches the forge their gates were published on
//! through [`StandInForge`], whose reviewers never answer.

pub mod asking;
pub mod beside;
pub mod ends;
pub mod past;
mod stand_ins;
pub mod standstills;
pub mod waiting;
pub mod work;

pub use stand_ins::StandInForge;

use std::path::{Path, PathBuf};

use yunta_api::Workspace;
use yunta_core::events::{EventDraft, EventPayload, NodeEvent, NodeStartedPayload, StoredEvent};
use yunta_core::{Manifest, NodeId, RunId};

use crate::designed::kept::Made;
use crate::designed::place::Place;
use crate::designed::script::{self, Step};

/// What every small run's id opens with: its last six characters, its handle,
/// name its scene.
const ID: &str = "01M7SMA11RVN00000000";

/// No session: no node of the run opens one.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// A change, then an external gate on a pull request of it.
const PUBLISHES: &str = r#"
name: published
nodes:
  - id: make
    kind: bash
    run: "echo made > made.txt"
  - id: approve
    kind: gate
    depends_on: [make]
    assignee: reviewer
    external:
      kind: pull_request
      artifacts: []
      branch: "{{run.branch}}"
"#;

/// A run woken once: a few seconds after its creation, with no one at its
/// terminal, until it parks or ends.
const WOKEN_ONCE: &[Step] = &[Step::Moves(2), Step::Wakes(&[])];

/// A small run, built: the place it was made in, kept while it is read, with
/// the runs its store holds, the scene's own first. Not
/// [`crate::designed::Recording`], the design's run, which is kept.
pub struct SmallRun {
    place: Place,
}

impl SmallRun {
    /// The run built in `place`.
    fn of(place: Place) -> SmallRun {
        SmallRun { place }
    }

    /// The scene's run.
    pub fn run(&self) -> &RunId {
        &self.place.bench.run_id
    }

    /// Every run the store holds: the scene's first, then each it made or that
    /// was made beside it, in the order the store lists them.
    pub fn runs(&self) -> Vec<RunId> {
        let listed = self
            .place
            .bench
            .storage
            .list_runs()
            .expect("the small run's store lists its runs");
        let mut runs = vec![self.run().clone()];
        runs.extend(
            listed
                .into_iter()
                .map(|listing| listing.run_id)
                .filter(|run| run != self.run()),
        );
        runs
    }

    /// Every event `run` appended, in log order.
    pub fn events(&self, run: &RunId) -> Vec<StoredEvent> {
        self.place
            .bench
            .storage
            .events_for_run(run)
            .expect("the small run's store reads its log")
    }

    /// The directory `run` keeps its manifest, its documents and its objects in.
    pub fn run_dir(&self, run: &RunId) -> PathBuf {
        self.place.bench.runs_root.join(run.as_str())
    }

    /// The directory the run's state is kept under: its store, its runs.
    pub fn root(&self) -> &Path {
        self.place.root()
    }

    /// The repository the run works in.
    pub fn repository(&self) -> &Path {
        &self.place.bench.worktree
    }

    /// A workspace in the run's repository, reading the state kept beside it.
    pub fn workspace(&self) -> Workspace {
        crate::sandbox::workspace(self.repository(), self.root())
    }

    /// Every run's log, one event a line, run after run, written as a recording
    /// keeps it: what two builds of one small run write alike.
    pub fn log(&self) -> String {
        let mut exported = String::new();
        let mut manifests = Vec::new();
        for run in self.runs() {
            for event in self.events(&run) {
                exported
                    .push_str(&serde_json::to_string(&event).expect("an event as a log keeps it"));
                exported.push('\n');
            }
            manifests.push(self.manifest(&run));
        }
        let made = Made {
            place: self.root().to_path_buf(),
            pool: self.place.bench.pool(),
            manifests,
        };
        made.log(&exported)
    }

    /// The manifest `run` froze.
    fn manifest(&self, run: &RunId) -> Manifest {
        let path = self.run_dir(run).join("manifest.yaml");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("the manifest `{}`: {error}", path.display()));
        Manifest::read_frozen(&bytes)
            .expect("a manifest the engine froze reads")
            .doc
    }
}

/// The id of the small run whose handle is `handle`.
fn id(handle: &str) -> String {
    format!("{ID}{handle}")
}

/// The run of `workflow` on `fixture` and `config` in `place`, created, then
/// woken once.
async fn woken(place: &Place, workflow: &str, fixture: &str, config: &str) {
    place
        .bench
        .create(workflow, &place.fixture(fixture), config)
        .await;
    script::played(place, WOKEN_ONCE).await;
}

/// A place of its own for the run whose handle is `handle`, its run of
/// `workflow` on `fixture` and `config` created, then woken once.
async fn woken_alone(handle: &str, workflow: &str, fixture: &str, config: &str) -> Place {
    let place = Place::of(&id(handle), &[], |bench| bench);
    woken(&place, workflow, fixture, config).await;
    place
}

/// `payload`, appended to the log of the run of `place` about `node`, or the
/// run itself, at the run's clock: what no wake of this binary writes.
fn appended(place: &Place, node: Option<&str>, payload: EventPayload) {
    let draft = EventDraft::new(place.bench.run_id.clone(), node.map(NodeId::from), payload)
        .expect("a draft names its subject");
    place
        .bench
        .storage
        .append(&draft, place.clock.as_ref())
        .expect("the log takes the event");
}

/// The start of `node` a crash left behind in the run of `place`: nothing after it says
/// how it ended.
fn started_by_a_crash(place: &Place, node: &str) {
    appended(
        place,
        Some(node),
        EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
    );
}
