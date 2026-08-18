use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use yunta_adapters::{Adapter, AgentEvent, Budget, MockAdapter, PermissionProfile, SessionRequest};
use yunta_core::{Capabilities, SessionId, YuntaError};

fn request(cwd: PathBuf) -> SessionRequest {
    SessionRequest {
        prompt: "do the thing".to_string(),
        cwd,
        model: None,
        agent: None,
        permissions: PermissionProfile::Edit,
        env: HashMap::new(),
        edit_constraints: None,
        budget: Budget::default(),
        adapter_settings: serde_json::Map::new(),
    }
}

async fn drain(mut session: Box<dyn yunta_adapters::AgentSession>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    let mut stream = session.events();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    events
}

#[tokio::test]
async fn a_successful_session_opens_then_completes() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        r#"
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
            input_tokens: 10,
            output_tokens: 5,
            ..
        }
    ));
    assert!(matches!(
        events.last().unwrap(),
        AgentEvent::Completed { result } if result.summary == "all good"
    ));
}

#[tokio::test]
async fn a_failed_session_ends_with_failed_and_its_retryable_flag() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        r#"
outcome: { type: failed, message: "criteria still red", retryable: true }
"#,
    )
    .unwrap();

    let session = fixture
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    let events = drain(session).await;

    assert!(matches!(events[0], AgentEvent::SessionOpened { .. }));
    match events.last().unwrap() {
        AgentEvent::Failed { error, retryable } => {
            assert_eq!(error.message, "criteria still red");
            assert!(retryable);
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn a_hung_session_never_produces_a_second_event_on_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml("outcome: { type: hang }").unwrap();

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
    let fixture = MockAdapter::from_yaml("outcome: { type: hang }").unwrap();

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
async fn an_out_of_scope_edit_is_blocked_when_the_adapter_has_edit_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        r#"
capabilities: { edit_hooks: true }
effects:
  - { path: src/lib.rs, content: "pub fn hello() {}\n" }
  - { path: outside/scope.rs, content: "should never land", blocked: true }
outcome: { type: completed, summary: "done" }
"#,
    )
    .unwrap();

    let session = fixture
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    let events = drain(session).await;

    assert!(dir.path().join("src/lib.rs").exists());
    assert!(!dir.path().join("outside/scope.rs").exists());

    let blocked_marker = events.iter().any(|e| {
        matches!(e, AgentEvent::ToolUse { name, target_digest }
            if name == "edit" && target_digest.contains("outside/scope.rs"))
    });
    assert!(
        blocked_marker,
        "expected a ToolUse marking the blocked edit"
    );
}

#[tokio::test]
async fn without_edit_hooks_the_engine_never_asked_for_the_constraint_is_not_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        r#"
capabilities: { edit_hooks: false }
effects:
  - { path: outside/scope.rs, content: "lands anyway", blocked: true }
outcome: { type: completed, summary: "done" }
"#,
    )
    .unwrap();

    let session = fixture
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    let _ = drain(session).await;

    // O5: without the capability, the adapter ignores the constraint
    // instead of failing — the engine's own post-check scope diff
    // (T5.3) is what would catch this later.
    assert!(dir.path().join("outside/scope.rs").exists());
}

#[tokio::test]
async fn probe_reports_healthy() {
    let fixture = MockAdapter::from_yaml("outcome: { type: completed, summary: ok }").unwrap();
    let report = fixture.probe().await.unwrap();
    assert!(report.healthy);
}

#[tokio::test]
async fn resume_defaults_to_unsupported() {
    let fixture = MockAdapter::from_yaml("outcome: { type: completed, summary: ok }").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let result = fixture
        .resume(
            &SessionId::from("some-session"),
            request(dir.path().to_path_buf()),
        )
        .await;

    match result {
        Err(YuntaError::Unsupported { adapter, what }) => {
            assert_eq!(adapter, "mock");
            assert_eq!(what, "resume_session");
        }
        Err(other) => panic!("expected Unsupported, got a different error: {other}"),
        Ok(_) => panic!("expected Unsupported, got a session"),
    }
}

#[tokio::test]
async fn declared_capabilities_come_straight_from_the_fixture() {
    let fixture = MockAdapter::from_yaml(
        r#"
capabilities: { resume_session: true, run_tools: true }
outcome: { type: completed, summary: ok }
"#,
    )
    .unwrap();
    let caps: Capabilities = fixture.capabilities();
    assert!(caps.resume_session);
    assert!(caps.run_tools);
    assert!(!caps.edit_hooks);
}
