//! What a session says goes only to a person at a terminal. A pipe ends up in
//! a CI log and `--detach` writes to a file, so nothing said reaches an
//! output that is not a terminal, nor a document a program reads.

use super::*;

/// A sentence nothing else in these runs carries.
const UNIQUE: &str = "OFF-TERMINAL-WORDS: I read the parser.";

/// The part of [`UNIQUE`] that shows whether any of it got out.
const MARK: &str = "OFF-TERMINAL-WORDS";

/// `yunta status <id> --follow` through a pipe, once it says why it writes
/// lines: the follower is following.
fn following_through_a_pipe(talking: &Talking, logs: &std::path::Path) -> std::process::Child {
    let err = logs.join("err");
    let mut follower = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    hermetic(&mut follower, &talking.repo, &talking.home);
    let follower = follower
        .args(["status", &talking.id(), "--follow"])
        .stdin(std::process::Stdio::null())
        .stdout(std::fs::File::create(logs.join("out")).unwrap())
        .stderr(std::fs::File::create(&err).unwrap())
        .spawn()
        .unwrap();
    let said = || std::fs::read_to_string(&err).unwrap_or_default();
    yunta_testkit::wait_until(
        || said().contains("not a terminal"),
        || format!("the follower never said why it writes lines: {}", said()),
    );
    follower
}

/// A run through a pipe, a follower through a pipe, and the documents a
/// program reads of the run carry nothing its session said, which a reader
/// of the run's socket heard.
///
/// Reached by: `crates/cli/src/surface/painter.rs`,
/// `crates/cli/src/commands/status/follow.rs`, `crates/cli/src/json/**`
#[test]
fn nothing_said_reaches_a_pipe_or_a_document() {
    let logs = tempfile::tempdir().unwrap();
    let mut talking = Talking::start(Speaker::Mock, &[UNIQUE]);
    let heard = yunta_testkit::said::heard_blocking(&talking.run_dir(), 1);
    assert_eq!(
        heard.first().map(|heard| heard.said.text.as_str()),
        Some(UNIQUE)
    );
    let mut follower = following_through_a_pipe(&talking, logs.path());

    let run = talking.stop();
    follower.wait().unwrap();

    assert!(!run.contains(MARK), "the run printed it: {run}");
    for stream in ["out", "err"] {
        let printed = std::fs::read_to_string(logs.path().join(stream)).unwrap();
        assert!(
            !printed.contains(MARK),
            "the follower printed it: {printed}"
        );
    }
    let id = talking.id();
    for asked in [
        vec!["status", id.as_str(), "--json"],
        vec!["receipt", id.as_str(), "--json"],
    ] {
        let document = yunta_in!(&talking.repo, &talking.home, &asked);
        let read = format!("{}{}", stdout(&document), stderr(&document));
        // A refusal would carry no words either, so the document has to be one.
        assert!(
            serde_json::from_str::<serde_json::Value>(&stdout(&document)).is_ok(),
            "`{}` gave no document: {read}",
            asked.join(" ")
        );
        assert!(
            !read.contains(MARK),
            "`{}` carries it: {read}",
            asked.join(" ")
        );
    }
}

/// A run on a terminal that says it is `dumb` writes lines, and none of them
/// carries what its session said.
///
/// Reached by: `crates/cli/src/surface/painter.rs`, `crates/cli/src/surface/mod.rs`
#[test]
fn nothing_said_reaches_a_dumb_terminal() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let home = root.path().join("state");
    project(&repo, Speaker::Mock, &[UNIQUE]);
    let mut terminal = yunta_testkit::yunta_on_terminal!(
        &repo,
        &home,
        &[
            "run",
            "wf.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml"
        ],
        &[("TERM", "dumb"), ("WORDS_TOKEN", SECRET)]
    );
    terminal.wait_for("TERM=dumb", "the run never said why it writes lines");
    let run_dir = runs_root(&home).join(wait_for(
        || {
            let run = std::fs::read_dir(runs_root(&home)).ok()?.flatten().next()?;
            Some(run.file_name())
        },
        || "the run never got a directory".to_string(),
    ));
    yunta_testkit::said::heard_blocking(&run_dir, 1);

    terminal.interrupt();
    let drawn = terminal.ended();

    assert!(!drawn.contains(MARK), "{drawn}");
}

/// A run handed over with `--detach` writes what its engine has to say to
/// `detached.log`, and nothing its session said.
///
/// Reached by: `crates/cli/src/surface/painter.rs`, `crates/cli/src/commands/run/detach.rs`
#[test]
fn nothing_said_reaches_a_detached_runs_log() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let home = root.path().join("state");
    project(&repo, Speaker::ClaudeCode, &[UNIQUE]);
    let held = [
        ("CLAUDE_STUB_LINGER".to_string(), "1".to_string()),
        ("WORDS_TOKEN".to_string(), SECRET.to_string()),
    ];
    let detached = yunta_in!(
        &repo,
        &home,
        &["run", "wf.yaml", "--detach", "--quiet"],
        &held
    );
    assert!(detached.status.success(), "{detached:?}");
    let id = stdout(&detached).trim().to_string();
    let run_dir = runs_root(&home).join(&id);
    let heard = yunta_testkit::said::heard_blocking(&run_dir, 1);
    assert_eq!(
        heard.first().map(|heard| heard.said.text.as_str()),
        Some(UNIQUE)
    );

    let cancel = yunta_in!(&repo, &home, &["cancel", &id]);
    assert!(cancel.status.success(), "{cancel:?}");

    let log = std::fs::read_to_string(run_dir.join("scratch").join("detached.log")).unwrap();
    assert!(!log.contains(MARK), "{log}");
}
