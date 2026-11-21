//! The design's own run of fragua, recorded and kept: one run through every
//! moment the design draws, written by the engine on the mock adapter, so a
//! target the application is held to stands on a log the engine wrote.
//!
//! The run asks a round and is answered, leaving its optional question
//! unanswered; has its plan approved; asks twice for a path outside its task's
//! scope and is granted it; has three reviewers at work at once posting the
//! design's four findings, and answers them; sends its lint back to `fix-lint`;
//! fails its suite, which a person's edit and a retry pass; is let through its
//! last gate; and opens its pull request on the forge `forge` stands in for.
//!
//! Two recordings give the same log. Every session is held by words, every
//! command the run measures by a `cargo` that holds when armed, and every node
//! started beside another at its first `git` call, before it writes anything
//! (`place`); `script` gives each word once the event the last one let go is
//! on the log, so nothing two sessions do races, and moves the clock past a
//! node at work beside others only once it gave its checkout back, so the pool
//! dates each give-back the same way. The run's clock stands still
//! until the recording moves it, by the seconds the design shows each node
//! working and each person taking: events between two moves share an instant.
//! Each pause is a park, answered as another process answers it, then a wake.
//!
//! What the log holds of the place and the machine it was recorded on is
//! written one way before it is kept (`kept`): the place's paths, the
//! machine's scratch directory, and how long the engine timed a command or a
//! rail call on the machine's own clock rather than the run's.
//!
//! The kept run stands in `fixtures/designed/`: its workflow, config and the
//! mock's fixture, which the recording reads; its log, `log.jsonl`; and its
//! run directory with its objects, `run/`. A recording names each file it
//! keeps, with what the file holds, so a test holds the kept run to it.
//!
//! [`crate::small_runs`] records each of its runs the same way, through
//! `place`, `script` and `kept`, and keeps none.

mod forge;
pub(crate) mod kept;
pub(crate) mod place;
pub(crate) mod script;

use std::path::{Path, PathBuf};

use kept::Made;
use place::Place;
use yunta_core::receipt::Receipt;
use yunta_core::RunId;

/// The design's run, by its id: its handle is the design's.
pub const RUN: &str = "01M7DES1GNED00000000Q78GRR";

/// What the person asked for.
pub const IDEA: &str = "add support for installing a pack globally";

/// The workflow the run is a run of.
const WORKFLOW: &str = include_str!("../fixtures/designed/workflow.yaml");

/// The config of the repository the run works in.
const CONFIG: &str = include_str!("../fixtures/designed/config.yaml");

/// What the mock's sessions do.
const FIXTURE: &str = include_str!("../fixtures/designed/fixture.yaml");

/// The kept log, under [`kept()`].
const LOG: &str = "log.jsonl";

/// The kept run directory, under [`kept()`].
pub const RUN_DIR: &str = "run";

/// Where the kept run stands.
pub fn kept() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/designed")
}

/// The kept log, one event a line, as the engine exports a run's log.
pub fn log() -> String {
    let path = kept().join(LOG);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the kept log `{}`: {error}", path.display()))
}

/// The design's run, recorded in a place of its own.
pub struct Recording {
    place: Place,
    made: Made,
    log: String,
}

/// Records the design's run again, from its creation to its end.
pub async fn record() -> Recording {
    let place = Place::made();
    place
        .bench
        .create(WORKFLOW, &place.fixture(FIXTURE), CONFIG)
        .await;
    script::played(&place, script::TIMELINE).await;
    let exported = tokio::fs::read_to_string(place.bench.run_dir().join("events.jsonl"))
        .await
        .expect("the run's exported log");
    let made = Made {
        place: place.root().to_path_buf(),
        pool: place.bench.pool(),
        manifests: vec![place.bench.manifest()],
    };
    let log = made.log(&exported);
    Recording { place, made, log }
}

impl Recording {
    /// The run's log, one event a line, written as it is kept.
    pub fn log(&self) -> &str {
        &self.log
    }

    /// Every file the recording keeps, by its path under [`kept()`], with what it
    /// holds: its log, and its run directory under `run/`.
    pub fn files(&self) -> Vec<(PathBuf, String)> {
        let run = Path::new(RUN_DIR);
        let mut files = vec![(PathBuf::from(LOG), self.log.clone())];
        files.extend(
            self.made
                .files(&self.place.bench.run_dir())
                .into_iter()
                .map(|(path, text)| (run.join(path), text)),
        );
        files
    }

    /// The recorded run's receipt, as `yunta receipt` writes it beside the run.
    pub async fn receipt(&self) -> Receipt {
        let workspace = crate::sandbox::workspace(&self.place.bench.worktree, self.place.root());
        let project = workspace.project().expect("the recording's repository");
        let run = project
            .run(&RunId::from(RUN))
            .await
            .expect("the recorded run");
        run.receipt().expect("the recorded run's receipt")
    }
}
