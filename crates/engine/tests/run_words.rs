//! What a session says reaches whoever watches its run, redacted before it
//! leaves the engine, and never the log: the observer of the engine's own
//! process, and the run's socket for any other.

mod common;
use common::*;

use yunta_core::events::{AgentMessageType, EventPayload, SessionEvent, StoredEvent};
use yunta_core::{NodeId, SessionId, TaskId};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, RecordingObserver, MOCK_CONFIG};

/// A value the run declares secret, which the session says back.
const SECRET: &str = "hunter2-the-whole-token";

/// The planner hands over one task, whose session says two things, the
/// second naming the secret, and does the task.
fn says_two_things() -> String {
    let mut fixture = plan_session(&independent_tasks(1));
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-1\"\n    steps:\n      - {{ type: note, text: \"I read the parser\" }}\n      - {{ type: note, text: \"ran psql with {SECRET}\" }}\n    effects:\n      - {{ path: out-1.txt, content: \"1\" }}\n    outcome: {{ type: completed, summary: \"did task-1\" }}\n"
    ));
    fixture
}

fn config() -> String {
    format!("{MOCK_CONFIG}\nsecrets: [YUNTA_TEST_TOKEN]\n")
}

/// Runs the one-task workflow on `bench`, the secret bound.
async fn run(bench: &Bench) {
    let RunReport { terminal, state } = bench
        .run_with_secrets(
            &concurrency_workflow(1),
            &says_two_things(),
            &config(),
            &[("YUNTA_TEST_TOKEN", SECRET)],
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");
}

/// The session `task` opened, as the log names it.
fn session_of(events: &[StoredEvent], task: &TaskId) -> SessionId {
    events
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Session(SessionEvent::Opened(opened)))
                if opened.task_id.as_ref() == Some(task) =>
            {
                Some(opened.session_id.clone())
            }
            _ => None,
        })
        .expect("the task's session opened")
}

/// The seqs of the notes `node` recorded, in order.
fn notes_of(events: &[StoredEvent], node: &NodeId) -> Vec<yunta_core::Seq> {
    events
        .iter()
        .filter(|event| event.node_id.as_ref() == Some(node))
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Session(SessionEvent::Message(message)))
                    if message.message_type == AgentMessageType::Note
            )
        })
        .map(|event| event.seq)
        .collect()
}

/// What a session says reaches the observer of the engine's own process,
/// each thing said once and in order, with the secret it named taken out and
/// the place of its note on the log. The log keeps only the note's digest,
/// and holds what the same run holds with nobody watching.
///
/// Reached by: `crates/engine/src/task_cycle/stream.rs`,
/// `crates/engine/src/run/ctx/observers.rs`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn what_a_session_says_reaches_the_observer_redacted_while_the_log_keeps_only_its_digest() {
    let recorder = RecordingObserver::new();
    let bench = Bench::new().with_observer(recorder.clone());
    run(&bench).await;

    let events = bench.events();
    let implement = NodeId::from("implement");
    let task = TaskId::from_static("task-1");
    let said = recorder.said();
    let texts: Vec<&str> = said.iter().map(|said| said.text.as_str()).collect();
    assert_eq!(texts, ["I read the parser", "ran psql with [redacted]"]);
    let session = session_of(&events, &task);
    for said in &said {
        assert_eq!(said.node_id, implement);
        assert_eq!(said.task_id.as_ref(), Some(&task));
        assert_eq!(said.session_id, session);
    }
    let seqs: Vec<_> = said.iter().map(|said| said.seq).collect();
    assert_eq!(seqs, notes_of(&events, &implement));

    let log = format!("{events:?}");
    assert!(
        !log.contains("I read the parser"),
        "the words reached the log"
    );
    assert!(!log.contains(SECRET), "the secret reached the log");
    let alone = Bench::new();
    run(&alone).await;
    let kinds = |events: Vec<StoredEvent>| -> Vec<String> {
        events
            .iter()
            .map(|event| event.body.kind_name().to_string())
            .collect()
    };
    assert_eq!(kinds(events), kinds(alone.events()));
}

/// The longest path a socket binds at on this host: its address, less the
/// closing NUL. Written here apart from the engine's own reading of it.
#[cfg(target_os = "linux")]
const SOCKET_PATH_MOST: usize = 107;
/// The longest path a socket binds at on this host: its address, less the
/// closing NUL. Written here apart from the engine's own reading of it.
#[cfg(not(target_os = "linux"))]
const SOCKET_PATH_MOST: usize = 103;

/// A session that says two things, the first in words nothing else in the
/// run carries, then holds until the run is cancelled.
const SAYS_THEN_HOLDS: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: note, text: "WORDS-ONLY-IN-MEMORY first" }
      - { type: note, text: "second" }
    outcome: { type: hang }
"#;

const TALKS: &str = r#"
name: talks
nodes:
  - { id: talk, kind: prompt, runner: executor, prompt: "Say what you do." }
"#;

/// A session that says one thing, then a node that holds until `go` exists
/// under `marks` and then ends with `exit`.
fn talks_then_holds(marks: &std::path::Path, exit: u8) -> String {
    let go = marks.join("go");
    format!(
        "name: talks-then-holds\nnodes:\n  - {{ id: talk, kind: prompt, runner: executor, prompt: \"Say it.\" }}\n  - {{ id: hold, kind: bash, depends_on: [talk], run: \"until [ -f {go} ]; do sleep 0.05; done; exit {exit}\" }}\n",
        go = go.display()
    )
}

const SAYS_ONE_THING: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: note, text: "one thing" }
    outcome: { type: completed, summary: said }
"#;

/// Where the socket of the run under `run_dir` goes on this host: its
/// scratch when that path fits, and the user's own directory under `/tmp`
/// otherwise. Worked out here from the run's directory alone.
fn expected_socket(run_dir: &std::path::Path, run: &str) -> std::path::PathBuf {
    use std::os::unix::fs::MetadataExt;
    let scratch = run_dir.join("scratch").join("live.sock");
    if scratch.as_os_str().len() <= SOCKET_PATH_MOST {
        return scratch;
    }
    // The bench's own directory was made by this process, so its owner is
    // the user this test runs as.
    let uid = std::fs::metadata(run_dir.parent().unwrap().parent().unwrap())
        .unwrap()
        .uid();
    std::path::Path::new("/tmp")
        .join(format!("yunta-{uid}"))
        .join(format!("{run}.sock"))
}

/// The files under `root` that hold `text`, the manifest that freezes a
/// mock's own script aside.
fn holding(root: &std::path::Path, text: &str) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                dirs.push(path);
            } else if kind.is_file()
                && path.file_name() != Some(std::ffi::OsStr::new("manifest.yaml"))
                && std::fs::read(&path)
                    .map(|bytes| String::from_utf8_lossy(&bytes).contains(text))
                    .unwrap_or(false)
            {
                found.push(path);
            }
        }
    }
    found
}

/// A reader of a run's socket gets what its session already said and then
/// what it says next, while the run moves. The run's registry names the
/// socket where it fits on this host, a cancel takes it away, and no file
/// under the run's home ever held the words.
///
/// Reached by: `crates/engine/src/run/invocation.rs`,
/// `crates/engine/src/process_registry/write.rs`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reader_of_a_runs_socket_gets_what_its_session_says() {
    let token = tokio_util::sync::CancellationToken::new();
    let bench = Bench::new().with_cancel(token.clone());
    let run_dir = bench.run_dir();

    let (report, (heard, socket)) = tokio::join!(bench.run(TALKS, SAYS_THEN_HOLDS), async {
        let heard = yunta_testkit::said::heard(&run_dir, 2).await;
        let socket = yunta_testkit::said::socket_of(&run_dir).await;
        token.cancel();
        (heard, socket)
    });

    let texts: Vec<&str> = heard.iter().map(|heard| heard.said.text.as_str()).collect();
    assert_eq!(texts, ["WORDS-ONLY-IN-MEMORY first", "second"]);
    assert_eq!(socket, expected_socket(&run_dir, bench.run_id.as_str()));
    assert!(
        matches!(report.terminal, RunTerminal::Paused { .. }),
        "{:?}",
        report.terminal
    );
    assert!(!socket.exists(), "the cancelled run left its socket");
    let home = bench.runs_root.parent().unwrap();
    assert_eq!(
        holding(home, "WORDS-ONLY-IN-MEMORY"),
        Vec::<std::path::PathBuf>::new()
    );
}

/// A run that finishes, and one that pauses on a failed node, each take
/// their socket away: it was there while they moved, and it is gone once
/// they stop.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_that_finishes_or_pauses_takes_its_socket_away() {
    for exit in [0, 1] {
        let marks = tempfile::tempdir().unwrap();
        let bench = Bench::new();
        let run_dir = bench.run_dir();
        let workflow = talks_then_holds(marks.path(), exit);

        let (report, socket) = tokio::join!(bench.run(&workflow, SAYS_ONE_THING), async {
            let socket = yunta_testkit::said::socket_of(&run_dir).await;
            std::fs::write(marks.path().join("go"), "go").unwrap();
            socket
        });

        let ended = match exit {
            0 => matches!(report.terminal, RunTerminal::Finished),
            _ => matches!(report.terminal, RunTerminal::Paused { .. }),
        };
        assert!(ended, "exit {exit}: {:?}", report.terminal);
        assert!(!socket.exists(), "exit {exit}: the run left its socket");
    }
}

/// The socket a crashed engine left behind is replaced by the next engine
/// for the run, which serves its readers there.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_next_engine_replaces_the_socket_a_crashed_one_left() {
    let marks = tempfile::tempdir().unwrap();
    let bench = Bench::new();
    let run_dir = bench.run_dir();
    let left = expected_socket(&run_dir, bench.run_id.as_str());
    // A socket nobody serves, left where this run's socket goes after the
    // run was born and before an engine drives it.
    let leave_a_socket = |_: &std::path::Path| {
        // Made the way the engine makes the user's own directory, so the
        // engine takes it for the user's alone when the socket goes there.
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(left.parent().unwrap())
            .unwrap();
        drop(std::os::unix::net::UnixListener::bind(&left).unwrap());
    };
    let workflow = talks_then_holds(marks.path(), 0);
    let driven = bench.run_sabotaged(&workflow, SAYS_ONE_THING, leave_a_socket);

    let (report, heard) = tokio::join!(driven, async {
        let heard = yunta_testkit::said::heard(&run_dir, 1).await;
        std::fs::write(marks.path().join("go"), "go").unwrap();
        heard
    });

    assert_eq!(report.terminal, RunTerminal::Finished);
    let texts: Vec<&str> = heard.iter().map(|heard| heard.said.text.as_str()).collect();
    assert_eq!(texts, ["one thing"]);
}
