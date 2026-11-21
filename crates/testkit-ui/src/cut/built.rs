//! What a cut is loaded from: the run a target shows, built ([`Built`]), with every run its
//! build made beside it ([`BuiltRun`]).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use yunta_core::events::StoredEvent;
use yunta_core::RunId;

use crate::designed::kept::PLACE;
use crate::small_runs::{asking, beside, ends, standstills, waiting, work, SmallRun};

/// A run the targets show: the design's run, kept, or one of the small runs, built where
/// it is read, each named for the function of [`crate::small_runs`] that builds it, and
/// `Past` for its two past runs of fragua. Not [`BuiltRun`], what building one makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Built {
    Designed,
    ConsoleGate,
    NotesGate,
    Departure,
    TokenCap,
    LoopCap,
    Failure,
    SentBack,
    Refused,
    Unread,
    NoMenu,
    NoRoom,
    Orphans,
    Blocked,
    Stopped,
    Aborted,
    Stalled,
    AskedElsewhere,
    WakesAlone,
    PullRequest,
    Child,
    Created,
    Reported,
    Failed,
    Promoted,
    Broken,
    TwoTasks,
    ModeSkips,
    Past,
    NotLoggedIn,
    LongStalled,
}

impl Built {
    /// The run, built: the design's read from where it is kept, a small run built here,
    /// on the heap, since a small run's build holds an engine's run.
    pub async fn build(self) -> BuiltRun {
        let small: Pin<Box<dyn Future<Output = SmallRun>>> = match self {
            Built::Designed => return BuiltRun::designed(),
            Built::ConsoleGate => Box::pin(asking::console_gate()),
            Built::NotesGate => Box::pin(asking::notes_gate()),
            Built::Departure => Box::pin(asking::departure()),
            Built::TokenCap => Box::pin(asking::token_cap()),
            Built::LoopCap => Box::pin(asking::loop_cap()),
            Built::Failure => Box::pin(asking::failure()),
            Built::SentBack => Box::pin(asking::sent_back()),
            Built::Refused => Box::pin(asking::refused()),
            Built::Unread => Box::pin(asking::unread()),
            Built::NoMenu => Box::pin(standstills::no_menu()),
            Built::NoRoom => Box::pin(standstills::no_room()),
            Built::Orphans => Box::pin(standstills::orphans()),
            Built::Blocked => Box::pin(standstills::blocked()),
            Built::Stopped => Box::pin(standstills::stopped()),
            Built::Aborted => Box::pin(standstills::aborted()),
            Built::Stalled => Box::pin(standstills::stalled()),
            Built::AskedElsewhere => Box::pin(standstills::asked_elsewhere()),
            Built::WakesAlone => Box::pin(waiting::wakes_alone()),
            Built::PullRequest => Box::pin(waiting::pull_request()),
            Built::Child => Box::pin(waiting::child()),
            Built::Created => Box::pin(ends::created()),
            Built::Reported => Box::pin(ends::reported()),
            Built::Failed => Box::pin(ends::failed()),
            Built::Promoted => Box::pin(ends::promoted()),
            Built::Broken => Box::pin(ends::broken()),
            Built::TwoTasks => Box::pin(work::two_tasks()),
            Built::ModeSkips => Box::pin(work::mode_skips()),
            Built::Past => Box::pin(crate::small_runs::past::runs()),
            Built::NotLoggedIn => Box::pin(beside::not_logged_in()),
            Built::LongStalled => Box::pin(beside::long_stalled()),
        };
        BuiltRun::small(small.await)
    }
}

/// A run built for a cut, with every run its build made beside it: what a cut loads. Not
/// [`SmallRun`], one of the small runs as it is built, nor the design's kept run.
pub struct BuiltRun {
    /// Every run the build made, the scene's first.
    pub(super) runs: Vec<RunId>,
    /// Every run's log, one event a line, as a recording keeps it.
    pub(super) log: String,
    /// Where each run's directory stands, in the order of `runs`.
    pub(super) dirs: Vec<PathBuf>,
    /// How the build names the place it was made in, in what a run keeps of it.
    places: Vec<String>,
    /// The `.yunta/` of the repository the runs were made in, where it holds one.
    pub(super) yunta: Option<PathBuf>,
    /// The small run, kept while its directories are read.
    _small: Option<SmallRun>,
}

impl BuiltRun {
    /// The design's run, as it is kept.
    fn designed() -> BuiltRun {
        BuiltRun {
            runs: vec![RunId::from(crate::designed::RUN)],
            log: crate::designed::log(),
            dirs: vec![crate::designed::kept().join(crate::designed::RUN_DIR)],
            places: vec![PLACE.to_string()],
            yunta: None,
            _small: None,
        }
    }

    /// `small`, as it was built.
    fn small(small: SmallRun) -> BuiltRun {
        let runs = small.runs();
        let dirs = runs.iter().map(|run| small.run_dir(run)).collect();
        let mut places = vec![PLACE.to_string()];
        if let Ok(resolved) = small.root().canonicalize() {
            places.push(resolved.display().to_string());
        }
        places.push(small.root().display().to_string());
        let yunta = small.repository().join(".yunta");
        BuiltRun {
            runs,
            log: small.log(),
            dirs,
            places,
            yunta: yunta.is_dir().then_some(yunta),
            _small: Some(small),
        }
    }

    /// The scene's run.
    pub fn run(&self) -> &RunId {
        &self.runs[0]
    }

    /// The seq the scene's run's log ends at.
    pub fn last(&self) -> u64 {
        self.events(self.run(), &self.log)
            .last()
            .map_or(0, |event| event.seq.get())
    }

    /// `run`'s events in `log`, the build's log as a place names it.
    pub(super) fn events(&self, run: &RunId, log: &str) -> Vec<StoredEvent> {
        log.lines()
            .map(|line| serde_json::from_str::<StoredEvent>(line).expect("an event of the log"))
            .filter(|event| &event.run_id == run)
            .collect()
    }

    /// `text` with every name of the place the build was made in naming `place`.
    pub(super) fn placed(&self, text: &str, place: &Path) -> String {
        let place = place.display().to_string();
        self.places
            .iter()
            .fold(text.to_string(), |text, made| text.replace(made, &place))
    }
}
