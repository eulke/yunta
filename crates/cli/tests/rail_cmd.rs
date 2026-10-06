//! `yunta rail <adapter-id> <event>` end to end: the real binary, a real
//! call on stdin, an engine answering over loopback, and the exit code
//! the calling CLI reads.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::mpsc;

use yunta_core::rail::{RailAct, RailAnswer, RailCall, TOKEN_VAR, URL_VAR};

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

/// The engine's rule: a write under `src/` goes on, anything else is
/// refused.
fn answer(call: Option<RailCall>) -> RailAnswer {
    match call.map(|call| call.act) {
        Some(RailAct::BeforeWrite { paths })
            if paths.iter().all(|path| {
                let path = path.to_string_lossy();
                path.starts_with("src/") || path.contains("/src/")
            }) =>
        {
            RailAnswer::Proceed
        }
        _ => RailAnswer::Refuse {
            reason: "yunta: write refused: docs/readme.md is outside this session's scope."
                .to_string(),
        },
    }
}

/// Runs the hook with `rail` in its environment and the call on stdin.
fn hook(rail: Option<(&str, &str)>, stdin: &str) -> std::process::Output {
    hook_for("claude-code", rail, stdin)
}

/// The same, for the hook of `adapter`.
fn hook_for(adapter: &str, rail: Option<(&str, &str)>, stdin: &str) -> std::process::Output {
    let away = tempfile::tempdir().expect("a directory for this test's home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_yunta"));
    yunta_testkit::hermetic(&mut command, away.path(), &away.path().join("state"));
    command
        .args(["rail", adapter, "before-write"])
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
