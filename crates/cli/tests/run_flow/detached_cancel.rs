//! A run cancelled the moment `--detach` hands it over: the engine it was
//! handed to registers a moment later, and the cancel stops it then.

use super::*;

/// The seconds the stub's `--version` takes. The engine a detached run is
/// handed to probes its adapter before it registers, so a cancel sent the
/// moment `--detach` returns lands before the registration every time.
const SLOW_VERSION: &str = "2";

/// What the stub streams once the session opens, before it lingers until
/// it is interrupted: a session that works while the cancel looks for it.
const STUB_LINES: &str = r#"{"type":"system","subtype":"init","session_id":"sess-cli","model":"claude-sonnet-5"}
"#;

/// A run cancelled as soon as `--detach` returns is cancelled: the cancel
/// waits for the engine it was handed to to register, as it waits for the
/// pause it asks for, instead of finding nothing to stop while the run goes
/// on without it.
#[test]
fn a_run_cancelled_as_soon_as_it_is_detached_is_cancelled() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(
        &repo.join(".yunta/config.yaml"),
        &format!(
            "runners:\n  executor:\n    - {{ adapter: claude-code, model: m }}\nadapters:\n  claude-code:\n    binary: {}\ndefaults:\n  timeout_minutes: 1\n",
            claude_code_stub().display()
        ),
    );
    write(
        &repo.join("wf.yaml"),
        "name: detached\nnodes:\n  - { id: a, kind: prompt, runner: executor, prompt: \"x\" }\n",
    );
    write(&repo.join(".claude-stub-lines.jsonl"), STUB_LINES);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);
    // The session lingers until the engine interrupts it, and the node's
    // one-minute timeout ends it if nothing does.
    let slow = [
        (
            "CLAUDE_STUB_VERSION_DELAY".to_string(),
            SLOW_VERSION.to_string(),
        ),
        ("CLAUDE_STUB_LINGER".to_string(), "1".to_string()),
    ];

    let detached = yunta_in!(
        &repo,
        &home,
        &["run", "wf.yaml", "--detach", "--quiet"],
        &slow
    );
    assert!(detached.status.success(), "{}", stderr(&detached));
    let run_id = stdout(&detached).trim().to_string();
    let cancel = yunta_in!(&repo, &home, &["cancel", &run_id]);

    let said = format!("{}{}", stdout(&cancel), stderr(&cancel));
    assert!(cancel.status.success(), "{said}");
    assert!(!said.contains("nothing to cancel"), "{said}");
    let status = stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    assert!(status.contains("cancelled"), "{status}");
}
