//! The row a person at a terminal reads for what a working node's session
//! said, attached and followed.

use super::*;

const FIRST: &str = "First I read the parser. Then the lexer.";
const NEXT: &str = "Now I add a test for the empty input.";

/// At a terminal, `yunta run` shows under its working node the first
/// sentence of what the node's session said, and when the session says
/// something else, the row says that instead.
///
/// Reached by: `crates/cli/src/surface/feed.rs`,
/// `crates/cli/src/surface/painter.rs`, `crates/cli/src/surface/region.rs`,
/// `crates/render/src/watched/heard.rs`
#[test]
fn attached_run_shows_what_its_session_says_under_its_node() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let home = root.path().join("state");
    project(&repo, Speaker::Mock, &[FIRST, NEXT]);

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
        &[("WORDS_TOKEN", SECRET)]
    );
    terminal.wait_on_screen(
        "says: First I read the parser.",
        "the region never showed what the session said",
    );
    terminal.wait_on_screen(
        "says: Now I add a test for the empty input.",
        "the row never changed to what the session said next",
    );
    assert!(
        !terminal.screen().holds("First I read the parser."),
        "the row kept what the session said before"
    );
    terminal.interrupt();
    terminal.ended();
}

/// The row of `screen` that says what a session said, as a reader reads it.
fn said_row(screen: &yunta_testkit::Screen) -> Option<String> {
    screen
        .rows
        .iter()
        .find(|row| row.contains("says:"))
        .map(|row| row.trim().to_string())
}

/// The id of the one run under `home`, once it has a directory.
fn the_run(home: &std::path::Path) -> String {
    let runs = runs_root(home);
    wait_for(
        || {
            let run = std::fs::read_dir(&runs).ok()?.flatten().next()?;
            Some(run.file_name().to_string_lossy().into_owned())
        },
        || "the run never got a directory".to_string(),
    )
}

/// `yunta status --follow` at a terminal shows the row the attached run shows
/// for the same session: what it had said before the follower came, read off
/// the run's socket, and what it says after.
///
/// Reached by: `crates/cli/src/commands/status/follow.rs`, `crates/api/src/run/follow/following.rs`,
/// `crates/api/src/run/follow/listening.rs`
#[test]
fn a_follower_shows_the_row_the_attached_run_shows() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let home = root.path().join("state");
    project(&repo, Speaker::Mock, &[FIRST, NEXT]);
    let mut attached = yunta_testkit::yunta_on_terminal!(
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
        &[("WORDS_TOKEN", SECRET)]
    );
    attached.wait_on_screen(
        "says: First I read the parser.",
        "the run never said its first thing",
    );

    let id = the_run(&home);
    let mut follower =
        yunta_testkit::yunta_on_terminal!(&repo, &home, &["status", &id, "--follow"]);
    follower.wait_on_screen(
        "says: First I read the parser.",
        "the follower never showed what was said before it came",
    );
    follower.wait_on_screen(
        "says: Now I add a test for the empty input.",
        "the follower never showed what was said after it came",
    );
    attached.wait_on_screen(
        "says: Now I add a test for the empty input.",
        "the run never said its next thing",
    );
    assert_eq!(said_row(&follower.screen()), said_row(&attached.screen()));
    attached.interrupt();
    attached.ended();
    follower.ended();
}

/// A follower that starts before the run's engine has bound its socket, as it
/// does on a run `--detach` has just handed over or one being resumed,
/// listens to the socket once the run's registry names it.
///
/// Reached by: `crates/cli/src/surface/region.rs`, `crates/render/src/watched/heard.rs`
#[test]
fn a_follower_listens_once_the_registry_names_a_socket() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let home = root.path().join("state");
    project(&repo, Speaker::ClaudeCode, &[FIRST]);
    // The engine a detached run is handed to probes its CLI before it
    // registers, so a slow `--version` holds the registry back while the
    // follower starts.
    let held = [
        ("CLAUDE_STUB_VERSION_DELAY".to_string(), "2".to_string()),
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
    let id = String::from_utf8_lossy(&detached.stdout).trim().to_string();

    let mut follower =
        yunta_testkit::yunta_on_terminal!(&repo, &home, &["status", &id, "--follow"]);
    follower.wait_on_screen(
        "says: First I read the parser.",
        "the follower never listened to the socket the registry named",
    );
    let cancel = yunta_in!(&repo, &home, &["cancel", &id]);
    assert!(cancel.status.success(), "{cancel:?}");
    follower.ended();
}
