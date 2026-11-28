//! A target screen's scene as the application meets it: each run it shows cut at one of
//! its moments and loaded into one fresh store, so the follower reads a cut as it reads
//! any log ([`Cut`]); and the committed table of every target screen's cut, by the
//! target's name ([`TARGET_CUTS`], in `targets`).
//!
//! A run is cut from what built it ([`Built`], in `built`): the design's run from its kept log, a
//! small run built in this process. Its log up to the cut is loaded at each event's own
//! seq and instant through `yunta-testkit`, which reaches the store; every other run the
//! same build made (a child, a successor, the other past runs) is loaded whole. Beside
//! each run its directory is copied as the design's run is kept, its working space empty
//! where the engine's scratch stood, its manifest naming the cut's repository, so the field places it here; or
//! another repository, or a manifest today's binary cannot read ([`Placed`]). The
//! registry its scene needs is written ([`Registered`]). The cut's repository holds what
//! the repositories its runs were made in hold under `.yunta/`: the workflows, the packs
//! and the config a new run reads. It is a repository set up for Yunta, as each one a run
//! was made in is, so the application opens on its field, which every run's screen stands
//! in: where none of them gave it a config, it holds one of its own.
//!
//! A cut's run goes on past its seq the same way: its next events loaded into the store
//! ([`Cut::goes_on`]), where the following reads them as it reads a run that moves. An
//! answer a key records is the application's own, written by its performer, and a run the
//! application hands on to a detached engine is handed to one that takes it up and leaves
//! its going on to the cut (`ENGINE`): the cut loads what that engine would write, after
//! what the key wrote, and registers the engine alive ([`Cut::driven`]).
//!
//! What a build leaves of the place it was made in names the cut's place once loaded: a
//! kept log names the place every recording is kept under, a small run's manifest the
//! place it was built in. The cut's clock stands at the latest instant one of its runs
//! stands at ([`Cut::now`]). A run at work, its engine alive, stands at its last event's
//! instant: it is seen as that event lands. Any other stands at the instant the event
//! after the cut came, or its last event's where its log ends at the cut, so a parked run
//! has waited as long as the person took to answer it. Its workspace reaches the forge its
//! runs' gates were published on, one whose reviewers never answer ([`StandInForge`]).

mod built;
mod targets;

pub use built::{Built, BuiltRun};
pub use targets::{
    Asked, HomePlace, Opens, Pointed, Stands, TargetCut, BESIDE, TARGET_CUTS, WITH_FRAGUA,
};

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tempfile::TempDir;
use yunta_api::Workspace;
use yunta_core::events::StoredEvent;
use yunta_core::{Clock, NodeId, RunId};
use yunta_testkit::WrittenRegistry;
use yunta_testkit_core::HostClock;

use crate::designed::kept::{KEPT, MANIFEST};
use crate::small_runs::StandInForge;

/// Where a cut's repository stands under its place: where a run's bench keeps the one it
/// works in, which its manifest names.
const REPOSITORY: &str = "worktree";

/// The store a workspace whose state is kept under the cut's place reads its log from.
const STORE: &str = "yunta.db";

/// Where such a workspace finds each run's directory, by the run's id.
const RUNS: &str = "runs";

/// The engine a workspace of the cut hands a run on to, under the cut's place: a program that
/// takes the run up and writes nothing, its going on loaded by the cut, as a detached
/// `yunta resume` would write it.
const ENGINE: &str = "engine";

/// What the engine a run is handed on to runs: it starts and ends at once, the run's going
/// on left to the cut.
const ENGINE_TAKES_IT_UP: &str = "#!/bin/sh\nexit 0\n";

/// Where a run of another repository names its repository, under the cut's place: a
/// directory no repository stands in.
const ELSEWHERE: &str = "elsewhere";

/// The schema a manifest today's binary cannot read is written at: a version far past
/// any this binary reads, as a newer binary that wrote it would.
const UNREAD_SCHEMA: u32 = 999;

/// Where a repository set up for Yunta keeps its config, under the repository.
const CONFIG: &str = ".yunta/config.yaml";

/// The config the cut's repository holds where none of its runs' repositories gave it
/// one: the least a repository set up for Yunta holds.
const SET_UP: &str = "defaults:\n  isolation: none\n";

/// What a cut registers of the engine driving the run it shows. Not replay's `Engine`,
/// what a reader makes of a registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registered {
    /// No registry: no engine drives the run, as after a pause or an end.
    Nothing,
    /// The engine that drove the run is gone: the run stalled.
    Gone,
    /// An engine in this process drives the run.
    Alive,
    /// An engine in this process drives the run and asks a person at its terminal about
    /// this node, since the instant of the run's last event.
    Asking(&'static str),
}

/// Where a run's manifest places it, beside the cut's repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placed {
    /// In the cut's repository: the field here holds it.
    Here,
    /// In another repository: only the field of the whole machine holds it.
    Elsewhere,
    /// Nowhere it can say: a newer binary wrote its manifest, and the run's branch in the
    /// cut's repository places it here, unread.
    Unread,
}

/// One run a cut shows: what builds it, the seq its log is cut at, what its registry
/// says of its engine, and where its manifest places it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub built: Built,
    pub seq: u64,
    pub registered: Registered,
    pub placed: Placed,
}

impl Shown {
    /// The run `built` makes, cut at `seq`, with no registry, here.
    pub const fn at(built: Built, seq: u64) -> Shown {
        Shown {
            built,
            seq,
            registered: Registered::Nothing,
            placed: Placed::Here,
        }
    }

    /// Driven by an engine in this process.
    pub const fn alive(self) -> Shown {
        Shown {
            registered: Registered::Alive,
            ..self
        }
    }

    /// Left by an engine that is gone.
    pub const fn gone(self) -> Shown {
        Shown {
            registered: Registered::Gone,
            ..self
        }
    }

    /// Driven by an engine in this process that asks a person at its terminal about
    /// `node`.
    pub const fn asking(self, node: &'static str) -> Shown {
        Shown {
            registered: Registered::Asking(node),
            ..self
        }
    }

    /// Made in another repository.
    pub const fn elsewhere(self) -> Shown {
        Shown {
            placed: Placed::Elsewhere,
            ..self
        }
    }

    /// With a manifest today's binary cannot read.
    pub const fn unread(self) -> Shown {
        Shown {
            placed: Placed::Unread,
            ..self
        }
    }
}

/// One run or several, each cut at one of its moments and loaded into one fresh store
/// beside its directory and the registry its scene needs, with a repository the field is
/// asked in: what the application meets when it opens over a target's runs.
pub struct Cut {
    /// The cut's place, kept while the cut is: its repository, its store and its runs'
    /// directories.
    _place: TempDir,
    /// The place as the file system resolves it, which every path of the cut names.
    root: PathBuf,
    /// Each run shown, in the order it was given.
    shown: Vec<RunId>,
    /// The instant the cut's clock stands at.
    now: DateTime<Utc>,
    /// The registries written, kept while the cut is.
    _registries: Vec<WrittenRegistry>,
    /// The forge its runs' gates were published on, which its workspace reaches.
    forge: StandInForge,
}

impl Cut {
    /// `shown`, each run built, cut and loaded into one fresh store.
    pub async fn of(shown: &[Shown]) -> Cut {
        let place = tempfile::tempdir().expect("a place for the cut");
        let root = place.path().canonicalize().expect("the cut's place");
        let repository = root.join(REPOSITORY);
        tokio::fs::create_dir_all(&repository)
            .await
            .expect("the cut's repository");
        yunta_testkit::init_repo(&repository);
        let mut cut = Cut {
            _place: place,
            root,
            shown: Vec::new(),
            now: yunta_testkit_core::fixed_now(),
            _registries: Vec::new(),
            forge: StandInForge::default(),
        };
        for each in shown {
            let built = each.built.build().await;
            let stands = cut.load(&built, each);
            cut.now = cut.now.max(stands);
            cut.shown.push(built.run().clone());
        }
        cut.set_up();
        cut.engine();
        cut
    }

    /// The engine a run is handed on to, written under the cut's place, ready to run.
    fn engine(&self) {
        let engine = self.root.join(ENGINE);
        std::fs::write(&engine, ENGINE_TAKES_IT_UP).expect("the engine a run is handed on to");
        let mut permissions = std::fs::metadata(&engine)
            .expect("the engine, written")
            .permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&engine, permissions).expect("the engine, made to run");
    }

    /// The cut's repository set up for Yunta, with a config of its own where none of its
    /// runs' repositories gave it one.
    fn set_up(&self) {
        let config = self.repository().join(CONFIG);
        if config.exists() {
            return;
        }
        let yunta = config.parent().expect("the config stands in `.yunta`");
        std::fs::create_dir_all(yunta).expect("the repository's `.yunta`");
        std::fs::write(&config, SET_UP).expect("the repository's config");
    }

    /// The cut of the target `name`, by its row of [`TARGET_CUTS`]. A target that opens in a
    /// place of the test home has no cut of its own: `Home::workspace_at` opens the place.
    pub async fn of_target(name: &str) -> Cut {
        let target = TARGET_CUTS
            .iter()
            .find(|target| target.name == name)
            .unwrap_or_else(|| panic!("no target is named `{name}`"));
        match target.stands {
            Stands::Run(_) | Stands::Field(_) => Cut::of(&target.stands.runs()).await,
            Stands::Home(place) => panic!("`{name}` opens in a place of the home, {place:?}"),
        }
    }

    /// Each run shown, in the order the cut was given them.
    pub fn runs(&self) -> &[RunId] {
        &self.shown
    }

    /// The instant the cut's clock stands at: the latest one of its runs stands at, a run
    /// at work at its last event's, any other at the event's after the cut.
    pub fn now(&self) -> DateTime<Utc> {
        self.now
    }

    /// The directory the cut's state is kept under: its store and its runs.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The repository the cut's field is asked in.
    pub fn repository(&self) -> PathBuf {
        self.root.join(REPOSITORY)
    }

    /// A workspace in the cut's repository, reading the state kept beside it, its clock
    /// standing at [`Cut::now`], reaching the forge its runs' gates were published on.
    pub fn workspace(&self) -> Workspace {
        let clock = HostClock::default();
        let ahead = (self.now - clock.now())
            .to_std()
            .expect("a cut stands after the instant every run starts at");
        clock.advance(ahead);
        Workspace {
            clock: Arc::new(clock),
            forges: Arc::new(self.forge.clone()),
            binary: self.root.join(ENGINE),
            ..crate::sandbox::workspace(&self.repository(), &self.root)
        }
    }

    /// How many times a workspace of the cut polled the forge its runs' gates were
    /// published on.
    pub fn forge_polls(&self) -> usize {
        self.forge.polls()
    }

    /// The scene's run of `shown` driven by an engine of this process from here on, as the
    /// engine it was handed on to drives it: its registry written alive.
    pub fn driven(&mut self, shown: Shown) {
        let run = &self.shown[0];
        let dir = self.root.join(RUNS).join(run.as_str());
        let alive = shown.alive();
        self.registered(&dir, &alive, self.now);
    }

    /// The scene's run of `shown` gone on: its events after `past` up to `to` loaded into
    /// the cut's store as `shown`'s cut loaded the ones before them, for a following to
    /// read as it reads a run that moves; and those events, as a following finds them.
    pub async fn goes_on(&self, shown: Shown, past: u64, to: u64) -> Vec<StoredEvent> {
        let built = shown.built.build().await;
        let log = built.placed(&built.log, &self.root);
        let after: Vec<StoredEvent> = built
            .events(built.run(), &log)
            .into_iter()
            .filter(|event| (past + 1..=to).contains(&event.seq.get()))
            .collect();
        yunta_testkit::loaded(&self.root.join(STORE), &after);
        after
    }

    /// `built`'s runs loaded as `shown` says: the scene's run cut at its seq, every other
    /// run whole, each beside its directory. Answers the instant the cut stands at for
    /// them.
    fn load(&mut self, built: &BuiltRun, shown: &Shown) -> DateTime<Utc> {
        let into = match shown.placed {
            Placed::Elsewhere => self.root.join(ELSEWHERE),
            Placed::Here | Placed::Unread => self.root.clone(),
        };
        let log = built.placed(&built.log, &self.root);
        let mut stands = yunta_testkit_core::fixed_now();
        for (at, run) in built.runs.iter().enumerate() {
            let whole = built.events(run, &log);
            let cut = match at {
                0 => shown.seq,
                _ => whole.last().map_or(0, |event| event.seq.get()),
            };
            let (kept, after): (Vec<StoredEvent>, Vec<StoredEvent>) =
                whole.into_iter().partition(|event| event.seq.get() <= cut);
            let last = kept.last().expect("a cut keeps the run's first event");
            let seen = match (at, shown.registered) {
                (0, Registered::Alive) => last,
                _ => after.first().unwrap_or(last),
            };
            stands = stands.max(seen.timestamp);
            yunta_testkit::loaded(&self.root.join(STORE), &kept);
            let dir = self.root.join(RUNS).join(run.as_str());
            self.copied(built, &built.dirs[at], &dir, &into);
            yunta_testkit::born_dirs(&dir);
            if at == 0 {
                self.registered(&dir, shown, last.timestamp);
                if shown.placed == Placed::Unread {
                    self.unread(run, &dir);
                }
            }
        }
        if let Some(yunta) = &built.yunta {
            copied_tree(yunta, &self.repository().join(".yunta"));
        }
        stands
    }

    /// What the cut keeps of the run directory `from` at `dir`, its manifest naming `into`
    /// as the place it was made in.
    fn copied(&self, built: &BuiltRun, from: &Path, dir: &Path, into: &Path) {
        for name in KEPT {
            let source = from.join(name);
            if !source.exists() {
                continue;
            }
            match name == MANIFEST {
                true => {
                    let manifest = std::fs::read_to_string(&source).expect("the run's manifest");
                    std::fs::create_dir_all(dir).expect("the run's directory");
                    std::fs::write(dir.join(MANIFEST), built.placed(&manifest, into))
                        .expect("the manifest, placed");
                }
                false => copied_tree(&source, &dir.join(name)),
            }
        }
    }

    /// The registry `shown` needs in the run directory `dir`, a question asked since `at`.
    fn registered(&mut self, dir: &Path, shown: &Shown, at: DateTime<Utc>) {
        let written = match shown.registered {
            Registered::Nothing => return,
            Registered::Gone => WrittenRegistry::left(dir),
            Registered::Alive => WrittenRegistry::driven(dir, None),
            Registered::Asking(node) => {
                WrittenRegistry::driven(dir, Some((&NodeId::from(node), at)))
            }
        };
        self._registries.push(written);
    }

    /// The run `run`, whose directory is `dir`, made unread: its manifest written at a
    /// schema today's binary refuses, its branch in the cut's repository placing it here.
    fn unread(&self, run: &RunId, dir: &Path) {
        let path = dir.join(MANIFEST);
        let manifest = std::fs::read_to_string(&path).expect("the run's manifest");
        let newer = manifest
            .lines()
            .map(|line| match line.starts_with("schema_version:") {
                true => format!("schema_version: {UNREAD_SCHEMA}"),
                false => line.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, newer).expect("the manifest, newer");
        yunta_testkit::git(&self.repository(), &["branch", &run.branch()]);
    }
}

/// Every file under `from` copied under `to`, directories made as they come.
fn copied_tree(from: &Path, to: &Path) {
    if from.is_file() {
        std::fs::create_dir_all(to.parent().expect("a file stands in a directory"))
            .expect("a directory of the cut");
        std::fs::copy(from, to).expect("a file of the run, copied");
        return;
    }
    for entry in std::fs::read_dir(from).expect("a directory of the run") {
        let entry = entry.expect("an entry of the run's directory");
        copied_tree(&entry.path(), &to.join(entry.file_name()));
    }
}
