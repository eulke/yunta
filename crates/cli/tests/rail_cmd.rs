//! `yunta rail <adapter-id> <event>` end to end: the real binary, a real
//! call on stdin, an engine answering over loopback, and the exit code
//! the calling CLI reads.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use yunta_core::fence::REFUSAL_MARKER;
use yunta_core::rail::answer::{FormatAnswer, FormatNote, StopAnswer, WriteAnswer, WriteRefusal};
use yunta_core::rail::window::{AskBudget, Wait};
use yunta_core::rail::{RailAct, RailCall, RailEndpoint, TOKEN_VAR, URL_VAR};
use yunta_core::{CommandName, Secret};
use yunta_testkit::after;

const TOKEN: &str = "the-session-token";

/// A `PreToolUse` call for the `claude-code` codec, writing `path`.
fn writing(path: &str) -> String {
    format!(r#"{{"session_id":"s-1","tool_name":"Write","tool_input":{{"file_path":"{path}"}}}}"#)
}

/// An engine that answers one call: it refuses any write outside `src/`,
/// and turns away a call without its session's credential. Returns where
/// it listens and what it was asked.
fn engine() -> (String, mpsc::Receiver<(String, Option<RailCall>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port for the engine");
    let url = format!("http://{}/rail", listener.local_addr().unwrap());
    let (asked, heard) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let (path, authorized, call) = request(&stream);
        let _ = asked.send((path, call.clone()));
        if !authorized {
            let _ = stream.write_all(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n");
            return;
        }
        let body = serde_json::to_string(&answer(call)).unwrap();
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
    });
    (url, heard)
}

/// What one request on `stream` asked: its path, whether it carried the
/// session's credential, and the call its body holds.
fn request(stream: &std::net::TcpStream) -> (String, bool, Option<RailCall>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    reader.read_line(&mut request_line).unwrap();
    let (mut length, mut authorized) = (0, false);
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(": ").unwrap_or_default();
        match name.to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse().unwrap(),
            "authorization" => authorized = value == format!("Bearer {TOKEN}"),
            _ => {}
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let path = request_line.split_whitespace().nth(1).unwrap_or_default();
    (
        path.to_string(),
        authorized,
        serde_json::from_slice(&body).ok(),
    )
}

/// The engine's rule at each moment: a write under `src/` goes on and any
/// other is refused; a write that landed is told what its formatter
/// changed; a stop is sent back to its work. Each answer is serialized the
/// way the engine sends it.
fn answer(call: Option<RailCall>) -> serde_json::Value {
    let value = match call.map(|call| call.act) {
        Some(RailAct::BeforeWrite { paths })
            if paths.iter().all(|path| {
                let path = path.to_string_lossy();
                path.starts_with("src/") || path.contains("/src/")
            }) =>
        {
            serde_json::to_value(WriteAnswer::Proceed)
        }
        Some(RailAct::AfterWrite { paths }) => serde_json::to_value(FormatAnswer::Tell {
            notes: vec![FormatNote::Changed {
                run: CommandName::from_static("rustfmt"),
                paths,
            }],
        }),
        Some(RailAct::BeforeStop) => serde_json::to_value(StopAnswer::Refuse {
            reason: "the work is not finished, so the session goes on.".to_string(),
        }),
        _ => serde_json::to_value(WriteAnswer::Refuse {
            reason: WriteRefusal::try_from(format!(
                "{REFUSAL_MARKER}docs/readme.md is outside this session's scope."
            ))
            .unwrap(),
        }),
    };
    value.unwrap()
}

/// Answers each connection on its own: a write answer waits `delay`, a
/// report is answered at once, and every request is recorded as it arrives.
fn slow_engine(delay: Duration) -> (String, mpsc::Receiver<(String, Option<RailCall>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port for the engine");
    let url = format!("http://{}/rail", listener.local_addr().unwrap());
    let (asked, heard) = mpsc::channel();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let (path, _, call) = request(&stream);
            let _ = asked.send((path.clone(), call.clone()));
            let wait = if path.ends_with("/report") {
                Duration::ZERO
            } else {
                delay
            };
            after(wait, move || {
                let body = serde_json::to_string(&answer(call)).unwrap();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    (url, heard)
}

/// Answers one request, then sends a byte every `every`, `ticks` times.
fn trickle(every: Duration, ticks: u32) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/rail", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = stream.read(&mut [0; 4096]);
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        for tick in 1..=ticks {
            let mut writer = stream.try_clone().unwrap();
            after(every * tick, move || {
                let _ = writer.write_all(b"x");
            });
        }
    });
    url
}

/// Runs the hook with `rail` in its environment and the call on stdin.
fn hook(rail: Option<(&str, &str)>, stdin: &str) -> std::process::Output {
    hook_for("claude-code", rail, stdin)
}

/// The same, for the write hook of `adapter`.
fn hook_for(adapter: &str, rail: Option<(&str, &str)>, stdin: &str) -> std::process::Output {
    hook_at(adapter, "before-write", rail, stdin)
}

/// The same, for `adapter`'s hook at `event`.
fn hook_at(
    adapter: &str,
    event: &str,
    rail: Option<(&str, &str)>,
    stdin: &str,
) -> std::process::Output {
    let away = tempfile::tempdir().expect("a directory for this test's home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_yunta"));
    yunta_testkit::hermetic(&mut command, away.path(), &away.path().join("state"));
    command
        .args(["rail", adapter, event])
        .env_remove(URL_VAR)
        .env_remove(TOKEN_VAR)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some((url, token)) = rail {
        command.env(URL_VAR, url).env(TOKEN_VAR, token);
    }
    let mut child = command.spawn().expect("the hook runs");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
        .expect("the call reaches the hook");
    child.wait_with_output().expect("the hook answers")
}

#[test]
fn a_refused_write_is_exit_two_with_the_engine_s_reason_on_stderr() {
    let (url, heard) = engine();

    let out = hook(Some((&url, TOKEN)), &writing("/work/task-1/docs/readme.md"));

    assert_eq!(out.status.code(), Some(2), "a refusal is exit 2");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains(yunta_core::fence::REFUSAL_MARKER) && said.contains("docs/readme.md"),
        "the model reads why, and what it tried: {said}"
    );
    let (path, call) = heard.recv().expect("the engine was asked");
    assert_eq!(path, "/rail/before-write");
    let call = call.expect("the call reads");
    assert_eq!(call.session_id.as_str(), "s-1");
    assert_eq!(
        call.act,
        RailAct::BeforeWrite {
            paths: vec!["/work/task-1/docs/readme.md".into()]
        }
    );
}

#[test]
fn an_allowed_write_is_exit_zero_with_nothing_on_stdout() {
    let (url, _heard) = engine();

    let out = hook(Some((&url, TOKEN)), &writing("/work/task-1/src/lib.rs"));

    assert_eq!(out.status.code(), Some(0), "consent is exit 0");
    assert!(
        out.stdout.is_empty(),
        "consent says nothing: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// A hook that cannot ask refuses; it never allows. One with no engine in
/// its environment, or one the engine turns away, has no idea what the
/// session may write.
#[test]
fn a_hook_that_cannot_ask_the_engine_refuses() {
    let out = hook(None, &writing("/work/task-1/src/lib.rs"));
    assert_eq!(out.status.code(), Some(2), "no engine to ask refuses");
    assert!(String::from_utf8_lossy(&out.stderr).contains("could not be asked"));

    let (url, _heard) = engine();
    let out = hook(Some((&url, "a-guess")), &writing("/work/task-1/src/lib.rs"));
    assert_eq!(out.status.code(), Some(2), "a turned-away call refuses");
}

#[test]
fn a_call_this_codec_cannot_read_refuses() {
    let (url, _heard) = engine();
    let out = hook(Some((&url, TOKEN)), r#"{"tool_name":"Write"}"#);
    assert_eq!(out.status.code(), Some(2));
}

/// A Codex patch asks about every file it names at once, and one path
/// outside the fence refuses the whole patch.
#[test]
fn a_codex_patch_with_one_path_outside_is_refused_whole() {
    let (url, heard) = engine();
    let patch = "*** Begin Patch\n*** Add File: src/a.rs\n+a\n*** Update File: docs/readme.md\n@@\n-x\n+y\n*** End Patch";
    let call = serde_json::json!({
        "session_id": "thread-1",
        "tool_name": "apply_patch",
        "tool_input": { "command": patch },
    })
    .to_string();

    let out = hook_for("codex", Some((&url, TOKEN)), &call);

    assert_eq!(out.status.code(), Some(2), "a refusal is exit 2");
    assert!(
        !out.stderr.is_empty(),
        "Codex reads an empty refusal as a failed hook"
    );
    let (_, call) = heard.recv().expect("the engine was asked");
    assert_eq!(
        call.expect("the call reads").act,
        RailAct::BeforeWrite {
            paths: vec!["src/a.rs".into(), "docs/readme.md".into()]
        }
    );
}

/// A stop the engine sends back keeps the session working: both CLIs read
/// the decision on stdout, with the engine's reason for the model.
#[test]
fn a_refused_stop_is_a_block_decision_on_stdout_for_every_adapter() {
    for adapter in ["claude-code", "codex"] {
        let (url, heard) = engine();
        let stop = r#"{"session_id":"s-1","hook_event_name":"Stop","stop_hook_active":false}"#;

        let out = hook_at(adapter, "before-stop", Some((&url, TOKEN)), stop);

        assert_eq!(out.status.code(), Some(0), "{adapter}");
        let said: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(said["decision"], "block", "{adapter}");
        assert!(said["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()));
        let (path, call) = heard.recv().expect("the engine was asked");
        assert_eq!(path, "/rail/before-stop");
        assert_eq!(call.map(|call| call.act), Some(RailAct::BeforeStop));
    }
}

/// A stop hook that cannot ask lets the session stop: the attempt's close
/// still judges the work.
#[test]
fn a_stop_hook_that_cannot_ask_lets_the_session_stop() {
    let stop = r#"{"session_id":"s-1","hook_event_name":"Stop"}"#;
    let out = hook_at("codex", "before-stop", None, stop);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
}

/// What the engine says after a write reaches the model as context, on
/// both CLIs; a hook after a write that cannot ask says nothing.
#[test]
fn what_the_engine_says_after_a_write_is_context_for_every_adapter() {
    for (adapter, call) in [
        ("claude-code", writing("/work/task-1/docs/readme.md")),
        (
            "codex",
            serde_json::json!({
                "session_id": "s-1",
                "tool_name": "apply_patch",
                "tool_input": { "command": "*** Begin Patch\n*** Add File: docs/readme.md\n+a\n*** End Patch" },
            })
            .to_string(),
        ),
    ] {
        let (url, heard) = engine();

        let out = hook_at(adapter, "after-write", Some((&url, TOKEN)), &call);

        assert_eq!(out.status.code(), Some(0), "{adapter}");
        let said: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(said["hookSpecificOutput"]["hookEventName"], "PostToolUse");
        assert!(said["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .is_some_and(|context| !context.is_empty()));
        let (path, _) = heard.recv().expect("the engine was asked");
        assert_eq!(path, "/rail/after-write");
    }
    let out = hook_at("claude-code", "after-write", None, &writing("a.rs"));
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
}

/// The window is one total, not one per read: a server that keeps the
/// socket busy with a byte every 300 ms cannot hold the ask past what is
/// left of its budget.
#[test]
fn an_ask_gives_up_when_its_window_is_spent_in_total() {
    let endpoint = RailEndpoint {
        url: trickle(Duration::from_millis(300), 16),
        token: Secret::new("t".to_string()),
    };
    let call = RailCall {
        session_id: "s-1".parse().unwrap(),
        act: RailAct::BeforeWrite {
            paths: vec![PathBuf::from("src/a.rs")],
        },
        model: None,
    };
    let started = Instant::now();
    let window = Wait::Write(AskBudget::from_process_start(
        started - Duration::from_millis(6_500),
    ));
    assert!(endpoint.ask::<WriteAnswer>(&call, window).is_err());
    assert!(
        started.elapsed() < Duration::from_millis(1_500),
        "a deadline per read waited out the trickle"
    );
}

/// A write whose engine does not answer inside its window is refused, and
/// the timeout reaches the log through one report.
#[test]
fn a_write_hook_past_its_window_refuses_and_reports_the_timeout() {
    let (url, heard) = slow_engine(Duration::from_secs(9));
    let out = hook(Some((&url, TOKEN)), &writing("src/lib.rs"));
    assert_eq!(out.status.code(), Some(2));
    let reports = heard
        .try_iter()
        .filter(|(path, _)| path.ends_with("/rail/report"))
        .count();
    assert_eq!(reports, 1, "the timeout reaches the log once");
}
