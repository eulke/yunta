//! The `mock` adapter and the forge beside it: a session is whatever the
//! fixture scripted for it, and nothing else.
//!
//! Every behavior a real adapter is asked for is exercised here against
//! a script — the terminal event a session always pays, the effects the
//! fence lets through, resume under one session id, the capabilities of
//! the CLI a fixture's dialect names, and a gate published to a pull
//! request — so the engine's own suites can trust the adapter they run on.

use std::path::{Path, PathBuf};
use std::time::Duration;
use yunta_core::fence::{Advice, Fence, Verdict};
use yunta_core::rail::answer::{FormatAnswer, StopAnswer, WriteAnswer, WriteRefusal};
use yunta_core::rail::{RailAct, RailCall, RailEndpoint};

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use futures::StreamExt;
use yunta_adapters::{
    ClaudeCodeAdapter, MockAdapter, MockFixture, MockForge, MockForgeState, RunPaths,
};
use yunta_core::events::{Claim, FailureCause, GateFolder, RefusalCause};
use yunta_core::port::{
    Adapter, AgentEvent, Forge, ForgeError, GateDecision, PermissionProfile, ProbeReport,
    PublishRequest, PullRequestRef, PullRequestRequest, ReviewOutcome, RunToolsEndpoint,
    SessionRequest,
};
use yunta_core::{
    AdapterSettings, Capabilities, Capability, Confinement, EditSandbox, FenceLevel, NodeId, RunId,
    RunTool, Secret, SessionId, ToolNaming,
};
use yunta_testkit_core::adapter::{confine, drain, request};

#[path = "mock/outcomes.rs"]
mod outcomes;

#[path = "mock/confinement.rs"]
mod confinement;

#[path = "mock/continuity.rs"]
mod continuity;

#[path = "mock/refusal.rs"]
mod refusal;

#[path = "mock/rail.rs"]
mod rail;

#[path = "mock/forge.rs"]
mod forge;

#[path = "mock/proof.rs"]
mod proof;

#[path = "mock/said.rs"]
mod said;

use rail::judged;

#[tokio::test]
async fn a_successful_session_opens_then_completes() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        r#"
dialect: claude-code
outcome: { type: completed, summary: "all good" }
steps:
  - { type: usage, input_tokens: 10, output_tokens: 5 }
"#,
    )
    .unwrap();

    let session = fixture
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    let events = drain(session).await;

    assert!(matches!(events[0], AgentEvent::SessionOpened { .. }));
    assert!(matches!(
        events[1],
        AgentEvent::Usage {
            input_tokens: Some(10),
            output_tokens: Some(5),
            ..
        }
    ));
    assert!(matches!(
        events.last().unwrap(),
        AgentEvent::Completed { result } if result.summary == "all good"
    ));
}

#[tokio::test]
async fn a_failed_session_ends_with_the_cause_its_message_classifies_to() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::from_yaml(
        r#"
dialect: claude-code
outcome: { type: failed, message: "401 Unauthorized" }
"#,
    )
    .unwrap();

    let events = drain(mock.spawn(request(dir.path().to_path_buf())).await.unwrap()).await;

    let Some(AgentEvent::Failed { cause, .. }) = events.last() else {
        panic!("a failed session: {events:?}")
    };
    assert_eq!(*cause, FailureCause::Refused(RefusalCause::Authentication));
}

/// A scripted session emits exactly one terminal event, and nothing after
/// it, whichever terminal outcome it scripts and whatever steps precede it.
#[tokio::test]
async fn a_scripted_session_ends_with_one_terminal_event_and_nothing_after_it() {
    for outcome in [
        "outcome: { type: completed, summary: all good }",
        "outcome: { type: failed, message: criteria still red }",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let fixture = MockAdapter::from_yaml(&format!(
            "dialect: claude-code\n{outcome}\nsteps:\n  - {{ type: note, text: one }}\n  - {{ type: usage, input_tokens: 1, output_tokens: 1 }}\n"
        ))
        .unwrap();
        let session = fixture
            .spawn(request(dir.path().to_path_buf()))
            .await
            .unwrap();
        let events = drain(session).await;

        let terminal = |event: &AgentEvent| {
            matches!(
                event,
                AgentEvent::Completed { .. } | AgentEvent::Failed { .. }
            )
        };
        assert_eq!(
            events.iter().filter(|event| terminal(event)).count(),
            1,
            "{events:?}"
        );
        assert!(terminal(events.last().unwrap()), "{events:?}");
    }
}

/// A declared cause the message does not classify to is refused, and the
/// refusal names both causes.
#[test]
fn a_declared_cause_the_message_does_not_classify_to_is_refused() {
    let refused = MockFixture::parse_without_a_run(
        r#"
dialect: claude-code
sessions:
  - outcome: { type: failed, message: "401 Unauthorized", cause: unreachable }
"#,
    )
    .expect_err("an unreachable cause on an auth failure is refused");
    let message = refused.to_string();
    assert!(
        message.contains("unreachable") && message.contains("authentication"),
        "the refusal names both causes: {message}"
    );
}

#[tokio::test]
async fn a_hung_session_never_produces_a_second_event_on_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml("dialect: claude-code\noutcome: { type: hang }").unwrap();

    let mut session = fixture
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    let mut stream = session.events();

    assert!(matches!(
        stream.next().await,
        Some(AgentEvent::SessionOpened { .. })
    ));
    let second = tokio::time::timeout(Duration::from_millis(50), stream.next()).await;
    assert!(
        second.is_err(),
        "a hung session must not produce a second event unprompted"
    );
}

#[tokio::test]
async fn killing_a_hung_session_before_draining_ends_the_stream_with_no_terminal_event() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml("dialect: claude-code\noutcome: { type: hang }").unwrap();

    let mut session = fixture
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    session.kill().await.unwrap();

    let events = tokio::time::timeout(Duration::from_millis(200), drain(session))
        .await
        .expect("kill should let the session wind down promptly");

    assert!(matches!(
        events.as_slice(),
        [AgentEvent::SessionOpened { .. }]
    ));
}

#[tokio::test]
async fn probe_reports_healthy() {
    let fixture =
        MockAdapter::from_yaml("dialect: claude-code\noutcome: { type: completed, summary: ok }")
            .unwrap();
    let report = fixture.probe().await.unwrap();
    assert!(matches!(report, ProbeReport::Healthy { .. }), "{report:?}");
}

#[tokio::test]
async fn resume_serves_the_next_script_under_the_same_session_id() {
    // The mock's resume is scripted like spawn, but the stream
    // reports the identity being continued — and records the ask, so
    // engine tests can prove the right conversation was picked up.
    let fixture =
        MockAdapter::from_yaml("dialect: claude-code\noutcome: { type: completed, summary: ok }")
            .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut session = fixture
        .resume(
            &SessionId::from("some-session"),
            request(dir.path().to_path_buf()),
        )
        .await
        .unwrap();

    let first = session.events().next().await.unwrap();
    match first {
        AgentEvent::SessionOpened { session_id, .. } => {
            assert_eq!(session_id, SessionId::from("some-session"));
        }
        other => panic!("expected SessionOpened first, got {other:?}"),
    }
    assert_eq!(
        fixture.resumes_seen(),
        vec![SessionId::from("some-session")]
    );
}

#[tokio::test]
async fn declared_capabilities_come_straight_from_the_dialect() {
    let fixture = MockAdapter::from_yaml(
        r#"
dialect: claude-code
outcome: { type: completed, summary: ok }
"#,
    )
    .unwrap();
    let caps: Capabilities = fixture.capabilities();
    assert!(caps.resume_session);
    assert!(caps.run_tools);
    assert_eq!(caps.fence, FenceLevel::ToolCalls);
    assert_eq!(caps.tool_naming, ToolNaming::McpPrefixed);
}

#[tokio::test]
async fn a_multi_session_fixture_scripts_each_spawn_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter::from_yaml(
        r#"
dialect: claude-code
sessions:
  - outcome: { type: completed, summary: "first session" }
  - outcome: { type: failed, message: "second session" }
"#,
    )
    .unwrap();

    let first = drain(
        adapter
            .spawn(request(dir.path().to_path_buf()))
            .await
            .unwrap(),
    )
    .await;
    assert!(matches!(
        first.last().unwrap(),
        AgentEvent::Completed { result } if result.summary == "first session"
    ));

    let second = drain(
        adapter
            .spawn(request(dir.path().to_path_buf()))
            .await
            .unwrap(),
    )
    .await;
    assert!(matches!(
        second.last().unwrap(),
        AgentEvent::Failed { error, cause: FailureCause::Refused(RefusalCause::Final) } if error.message == "second session"
    ));
}

#[tokio::test]
async fn an_exhausted_fixture_refuses_further_spawns_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter::from_yaml(
        r#"
dialect: claude-code
sessions:
  - outcome: { type: completed, summary: "only one" }
"#,
    )
    .unwrap();

    let _ = adapter
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    match adapter.spawn(request(dir.path().to_path_buf())).await {
        Ok(_) => panic!("expected the exhausted fixture to refuse the spawn"),
        Err(err) => assert!(err.to_string().contains("exhausted"), "got: {err}"),
    }
}

#[tokio::test]
async fn each_scripted_session_applies_only_its_own_effects() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter::from_yaml(
        r#"
dialect: claude-code
sessions:
  - effects: [{ path: first.txt, content: "1" }]
    outcome: { type: completed, summary: "one" }
  - effects: [{ path: second.txt, content: "2" }]
    outcome: { type: completed, summary: "two" }
"#,
    )
    .unwrap();

    drain(
        adapter
            .spawn(judged(request(dir.path().to_path_buf()), &adapter).await)
            .await
            .unwrap(),
    )
    .await;
    assert!(dir.path().join("first.txt").exists());
    assert!(!dir.path().join("second.txt").exists());

    drain(
        adapter
            .spawn(judged(request(dir.path().to_path_buf()), &adapter).await)
            .await
            .unwrap(),
    )
    .await;
    assert!(dir.path().join("second.txt").exists());
}

#[tokio::test]
async fn a_single_session_fixture_still_scripts_exactly_one_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let adapter =
        MockAdapter::from_yaml("dialect: claude-code\noutcome: { type: completed, summary: ok }")
            .unwrap();

    let _ = adapter
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    match adapter.spawn(request(dir.path().to_path_buf())).await {
        Ok(_) => panic!("expected the exhausted fixture to refuse the spawn"),
        Err(err) => assert!(err.to_string().contains("exhausted"), "got: {err}"),
    }
}

fn prompt_request(cwd: PathBuf, prompt: &str) -> SessionRequest {
    SessionRequest {
        prompt: prompt.to_string(),
        ..request(cwd)
    }
}

#[tokio::test]
async fn match_prompt_contains_picks_the_right_script_out_of_call_order() {
    // Concurrent task dispatch means spawn() calls no longer land
    // in fixture-declaration order — a script that names which request
    // it belongs to must be selectable regardless of when it's called.
    // Each request gets its OWN cwd (as real per-task worktrees would),
    // so the assertions can tell whose effect actually landed where.
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let fixture = r#"
dialect: claude-code
sessions:
  - match_prompt_contains: "task-b"
    effects:
      - { path: b.txt, content: "b" }
    outcome: { type: completed, summary: did-b }
  - match_prompt_contains: "task-a"
    effects:
      - { path: a.txt, content: "a" }
    outcome: { type: completed, summary: did-a }
"#;
    let adapter = MockAdapter::from_yaml(fixture).unwrap();

    // Spawned in the OPPOSITE order the fixture declares them.
    let prompt_a = prompt_request(dir_a.path().to_path_buf(), "do task-a now");
    let session_a = adapter
        .spawn(judged(prompt_a, &adapter).await)
        .await
        .unwrap();
    drain(session_a).await;
    let prompt_b = prompt_request(dir_b.path().to_path_buf(), "do task-b now");
    let session_b = adapter
        .spawn(judged(prompt_b, &adapter).await)
        .await
        .unwrap();
    drain(session_b).await;

    assert!(
        dir_a.path().join("a.txt").exists(),
        "task-a's request must get task-a's own script, not whatever spawned first"
    );
    assert!(
        !dir_a.path().join("b.txt").exists(),
        "task-a's cwd must never see task-b's effect"
    );
    assert!(dir_b.path().join("b.txt").exists(), "task-b's own effect");
    assert!(!dir_b.path().join("a.txt").exists());
}

#[tokio::test]
async fn a_matched_script_is_never_consumed_twice() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = r#"
dialect: claude-code
sessions:
  - match_prompt_contains: "task-a"
    outcome: { type: completed, summary: did-a }
"#;
    let adapter = MockAdapter::from_yaml(fixture).unwrap();

    let first = adapter
        .spawn(prompt_request(dir.path().to_path_buf(), "do task-a"))
        .await
        .unwrap();
    drain(first).await;

    match adapter
        .spawn(prompt_request(dir.path().to_path_buf(), "do task-a again"))
        .await
    {
        Ok(_) => panic!("the matching script was already consumed"),
        Err(err) => assert!(err.to_string().contains("exhausted"), "got: {err}"),
    }
}

#[tokio::test]
async fn unmatched_scripts_still_serve_in_declaration_order() {
    // Fixtures that never set match_prompt_contains keep their exact
    // behavior — this field is additive, not a breaking change.
    let dir = tempfile::tempdir().unwrap();
    let fixture = r#"
dialect: claude-code
sessions:
  - effects:
      - { path: first.txt, content: "1" }
    outcome: { type: completed, summary: first }
  - effects:
      - { path: second.txt, content: "2" }
    outcome: { type: completed, summary: second }
"#;
    let adapter = MockAdapter::from_yaml(fixture).unwrap();

    drain(
        adapter
            .spawn(judged(request(dir.path().to_path_buf()), &adapter).await)
            .await
            .unwrap(),
    )
    .await;
    assert!(dir.path().join("first.txt").exists());
    drain(
        adapter
            .spawn(judged(request(dir.path().to_path_buf()), &adapter).await)
            .await
            .unwrap(),
    )
    .await;
    assert!(dir.path().join("second.txt").exists());
}
