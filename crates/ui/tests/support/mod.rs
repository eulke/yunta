//! What the tests of the application share: a workspace made in a test's own
//! directory, which reads nothing of the machine's and finds only the CLIs a test
//! stands in for on its `PATH`, a state driven with a performer
//! until it waits on nothing, a piece of work performed in a bench's world, and the
//! application open on a field of runs a test makes up.
//!
//! Every test binary that declares `mod support` compiles the whole module and uses a
//! part of it, so what another binary needs reads as dead here.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use surco::words::Standing;
use tokio::sync::mpsc;
use yunta_api::{FieldReach, FieldReading, Remark, Repository, Stop, Workspace};
use yunta_core::events::PauseReason;
use yunta_core::process::SearchPath;
use yunta_core::{Env, ProcessSecrets, RunId, SystemIdSource};
use yunta_render::page::FieldRow;
use yunta_replay::frame::{RunFrame, RunPhase, WaitingOn};
use yunta_testkit::Bench;
use yunta_testkit_core::FixedClock;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, News, Operation, Outcome, Performer, Request, State};

/// A workspace working in `dir`, its state kept in `root` and its org layer there too,
/// and a `PATH` git stands on, which holds no adapter's CLI.
pub fn workspace(dir: &Path, root: &Path) -> Workspace {
    workspace_on(dir, root, Vec::new())
}

/// A workspace as [`workspace`] makes it, whose `PATH` holds `first` before git's
/// directories: where a test puts the CLIs it stands in for.
pub fn workspace_on(dir: &Path, root: &Path, first: Vec<PathBuf>) -> Workspace {
    let mut dirs = first;
    dirs.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
    let env = Env {
        yunta_home: Some(root.to_path_buf()),
        org_config: Some(root.join("org.yaml")),
        path: SearchPath::new(dirs),
        ..Env::default()
    };
    Workspace {
        dir: dir.to_path_buf(),
        env,
        clock: Arc::new(FixedClock),
        ids: Arc::new(SystemIdSource),
        secrets: Arc::new(ProcessSecrets),
        binary: PathBuf::from("yunta"),
    }
}

/// A directory under `root` holding a `claude` that answers `--version` as Claude
/// Code does: on a workspace's `PATH`, the adapter this machine answers for. It opens
/// no session.
pub fn claude_answering(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).expect("a directory for the CLIs");
    let claude = bin.join("claude");
    std::fs::write(&claude, "#!/bin/sh\necho '2.1.0 (Claude Code)'\n").expect("a stub CLI");
    let executable = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&claude, executable).expect("a stub CLI that runs");
    bin
}

/// A pack's manifest: one workflow, nothing it executes.
const MANIFEST: &str = "name: demo\npublisher: acme\nversion: 1.0.0\ndeclares:\n  \
                        permissions: read-only\n  network: false\n  executors: []\n\
                        contents:\n  workflows: [workflows/demo-says.yaml]\n";

/// The pack's one workflow.
const DEMO_SAYS: &str = "name: demo-says\nnodes:\n  - { id: say, kind: bash, run: \"true\" }\n";

/// A repository set up for Yunta under `root`, with `acme/demo` installed from a
/// local repository beside it.
pub async fn with_a_pack_installed(root: &Path) -> PathBuf {
    let upstream = root.join("upstream");
    std::fs::create_dir_all(upstream.join("workflows")).expect("the pack's directory");
    yunta_testkit::init_repo(&upstream);
    std::fs::write(upstream.join("pack.yaml"), MANIFEST).expect("its manifest");
    std::fs::write(upstream.join("workflows/demo-says.yaml"), DEMO_SAYS).expect("its workflow");
    yunta_testkit::git(&upstream, &["add", "."]);
    yunta_testkit::git(&upstream, &["commit", "-q", "-m", "v1"]);
    let repo = root.join("project");
    std::fs::create_dir_all(repo.join(".yunta")).expect("its .yunta");
    yunta_testkit::init_repo(&repo);
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    )
    .expect("its config");
    let source = upstream.to_str().expect("a path that reads");
    let stop = Stop::new();
    let remarks = |_: Remark| {};
    let installing = workspace(&repo, root);
    let fetched = installing
        .packs()
        .fetch(source, false, &stop)
        .await
        .expect("the pack fetched");
    fetched
        .install(&remarks, &stop)
        .await
        .expect("the pack installed");
    repo
}

/// A local repository under `root` holding the pack `acme/tools`, which ships the
/// executor `runner.py`, committed once; and the commit it stands at.
pub fn pack_with_an_executor(root: &Path) -> (PathBuf, String) {
    let upstream = root.join("tools");
    std::fs::create_dir_all(upstream.join("workflows")).expect("the pack's directory");
    yunta_testkit::init_repo(&upstream);
    let manifest = "name: tools\npublisher: acme\nversion: 1.0.0\ndeclares:\n  \
                    permissions: read-only\n  network: false\n  executors: [runner.py]\n\
                    contents:\n  workflows: [workflows/noop.yaml]\n";
    std::fs::write(upstream.join("pack.yaml"), manifest).expect("its manifest");
    let noop = "name: noop\nnodes:\n  - { id: noop, kind: bash, run: \"true\" }\n";
    std::fs::write(upstream.join("workflows/noop.yaml"), noop).expect("its workflow");
    std::fs::write(upstream.join("runner.py"), "print('runs')\n").expect("its executor");
    yunta_testkit::git(&upstream, &["add", "."]);
    yunta_testkit::git(&upstream, &["commit", "-q", "-m", "v1"]);
    let head = commit_of(&upstream);
    (upstream, head)
}

/// The commit `repo` stands at.
pub fn commit_of(repo: &Path) -> String {
    yunta_testkit::git_output(repo, &["rev-parse", "HEAD"])
        .trim()
        .to_string()
}

/// The instant the application stands at in a test.
pub fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000, 0).expect("an instant")
}

/// `state` driven with `performer`: what `asked` asks for and every request it leads to
/// carried out, each answer handed back, then each of `strokes` in turn once the state
/// waits on nothing; the state as it stands when it waits on nothing and no stroke is
/// left, with what only a surface does, showing a page or leaving, that it asked for.
pub async fn drive(
    mut state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    strokes: &[Stroke],
) -> (State, Vec<Request>) {
    let (inputs, mut received) = mpsc::unbounded_channel();
    let mut strokes = strokes.iter().copied();
    let (mut asked, mut surfaced) = (asked, Vec::new());
    loop {
        for request in asked.drain(..) {
            surfaced.extend(performer.take(request, &inputs));
        }
        if state.waiting_on().is_empty() {
            match strokes.next() {
                Some(stroke) => asked = state.update(Input::Stroke(stroke)),
                None => return (state, surfaced),
            }
            continue;
        }
        let input = received
            .recv()
            .await
            .expect("the performer hands back what the state waits on");
        asked = state.update(input);
    }
}

/// A run a person stopped and nobody closed, which a field lists under what is stopped.
pub fn stopped(run: &RunId) -> FieldRow {
    FieldRow {
        frame: RunFrame {
            standing: Standing::Stopped,
            phase: RunPhase::Waiting {
                on: WaitingOn::Run {
                    reason: PauseReason::Cancelled,
                },
            },
            ..RunFrame::sample(run)
        },
        menu: false,
    }
}

/// The application open on a field of `rows`.
pub fn on_the_field(rows: Vec<FieldRow>) -> State {
    let (mut state, asked) = State::opened(now());
    let [Request::Perform(opening, _)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    let asked = state.update(Input::Outcome(
        *opening,
        Outcome::Repository(Repository::SetUp),
    ));
    let [Request::Perform(
        reading,
        Operation::Field {
            reach: FieldReach::Here,
        },
    )] = asked.as_slice()
    else {
        panic!("in a repository the field is read: {asked:?}");
    };
    let reading_outcome = Outcome::Field {
        reading: FieldReading {
            rows,
            unreadable: Vec::new(),
            elsewhere: 0,
            here: true,
        },
        storage: PathBuf::from("/home/me/.yunta"),
    };
    state.update(Input::Outcome(*reading, reading_outcome));
    state
}

/// What `operation` hands back, performed in `bench`'s world: what it finds while it
/// lasts, then what it comes to.
pub async fn performed(bench: &Bench, operation: Operation) -> (Vec<News>, Outcome) {
    let root = bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let performer = Performer::new(workspace(&bench.worktree, root));
    // A ticket of the application's own: the performer hands back under what it is given.
    let (_, requests) = State::opened(now());
    let Some(Request::Perform(ticket, _)) = requests.into_iter().next() else {
        panic!("the application asks something as it opens");
    };
    let (inputs, mut received) = mpsc::unbounded_channel();
    performer.perform(ticket, operation, inputs).await;
    let mut found = Vec::new();
    loop {
        match received.recv().await {
            Some(Input::News(_, news)) => found.push(news),
            Some(Input::Outcome(_, outcome)) => return (found, outcome),
            Some(_) => continue,
            None => panic!("the performer hands back an outcome"),
        }
    }
}
