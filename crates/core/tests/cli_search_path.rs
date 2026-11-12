//! Where a CLI named without a directory is found: on the search path its probe
//! or its launch is handed, the environment's, and never on the process's own
//! `PATH`, so what a probe vouches for is what the session runs.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use yunta_core::port::session::SessionEnv;
use yunta_core::port::{AgentEvent, ProbeReport};
use yunta_core::process::mark::SessionMark;
use yunta_core::process::subprocess::{self, probe_version, Launch, LineParser};
use yunta_core::process::SearchPath;
use yunta_core::{AdapterId, RunId};

/// A parser no line reaches: the launches here never start.
struct ReadsNothing;

impl LineParser for ReadsNothing {
    fn parse(&mut self, _line: &str) -> Vec<AgentEvent> {
        Vec::new()
    }
}

/// A directory holding an `sh` that answers `--version` with `stub-sh 1.0`.
fn stub_sh() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let sh = dir.path().join("sh");
    std::fs::write(&sh, "#!/bin/sh\necho stub-sh 1.0\n").unwrap();
    std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = dir.path().to_path_buf();
    (dir, path)
}

/// `sh` stands on the process's own `PATH`; a search path that holds no `sh`
/// does not find it there.
#[tokio::test]
async fn a_cli_on_the_process_path_alone_is_not_found() {
    let report = probe_version(Path::new("sh"), &SearchPath::default()).await;
    assert_eq!(
        report,
        ProbeReport::Unhealthy {
            diagnostic: "`sh` is not on PATH".to_string()
        }
    );
}

/// The CLI asked is the one the search path holds, not the system's.
#[tokio::test]
async fn the_cli_asked_is_the_one_the_search_path_holds() {
    let (_dir, holding) = stub_sh();
    let report = probe_version(Path::new("sh"), &SearchPath::new(vec![holding])).await;
    assert_eq!(
        report,
        ProbeReport::Healthy {
            version: Some("stub-sh 1.0".to_string())
        }
    );
}

/// A session's launch looks its CLI up where its probe does: one the search
/// path does not hold is refused before anything starts.
#[tokio::test]
async fn a_launch_finds_its_cli_where_the_probe_does() {
    static ADAPTER: AdapterId = AdapterId::from_static("probe");
    let dir = tempfile::tempdir().unwrap();
    let env = SessionEnv::default();
    let mark = SessionMark::new(&RunId::from("01HZYXW8M6D4V2T0R9Q7P5N3K6"));
    let opened = subprocess::open(Launch {
        adapter: &ADAPTER,
        binary: Path::new("sh"),
        search_path: &SearchPath::default(),
        args: Vec::new(),
        cwd: dir.path(),
        env: &env,
        mark: &mark,
        prompt: "",
        parser: Box::new(ReadsNothing),
        held: None,
    })
    .await;
    let Err(refused) = opened else {
        panic!("a CLI the search path does not hold is not launched");
    };
    assert!(
        yunta_core::describe(&refused).contains("`sh` is not on PATH"),
        "{}",
        yunta_core::describe(&refused)
    );
}

/// A CLI named by a path relative to where the session runs, with a directory in it,
/// is the file there, wherever the process itself stands: it is found and started.
#[tokio::test]
async fn a_cli_named_by_a_relative_path_is_the_one_where_the_session_runs() {
    static ADAPTER: AdapterId = AdapterId::from_static("probe");
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("bin")).unwrap();
    let cli = dir.path().join("bin/fake");
    std::fs::write(&cli, "#!/bin/sh\ncat >/dev/null\n").unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_ne!(
        std::env::current_dir().unwrap(),
        dir.path(),
        "the process stands elsewhere"
    );
    let env = SessionEnv::default();
    let mark = SessionMark::new(&RunId::from("01HZYXW8M6D4V2T0R9Q7P5N3K7"));
    let opened = subprocess::open(Launch {
        adapter: &ADAPTER,
        binary: Path::new("bin/fake"),
        search_path: &SearchPath::default(),
        args: Vec::new(),
        cwd: dir.path(),
        env: &env,
        mark: &mark,
        prompt: "",
        parser: Box::new(ReadsNothing),
        held: None,
    })
    .await;
    if let Err(refused) = opened {
        panic!("{}", yunta_core::describe(&refused));
    }
}
