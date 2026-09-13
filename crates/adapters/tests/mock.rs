use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use yunta_adapters::{
    Adapter, AgentEvent, Budget, Forge, ForgeError, MockAdapter, MockForge, MockForgeState,
    PermissionProfile, ProbeReport, PublishRequest, PublishedGate, ReviewOutcome, SessionRequest,
};
use yunta_core::{Capabilities, SessionId};

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
        skills: Vec::new(),
        run_tools_endpoint: None,
        artifact_dir: None,
        scratch_dir: None,
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
async fn blocked_is_derived_from_edit_constraints() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        r#"
capabilities: { edit_hooks: true }
effects:
  - { path: src/lib.rs, content: "pub fn hello() {}\n" }
  - { path: outside/scope.rs, content: "should never land" }
outcome: { type: completed, summary: "done" }
"#,
    )
    .unwrap();

    let mut req = request(dir.path().to_path_buf());
    req.edit_constraints = Some(vec!["src/**".to_string()]);
    let session = fixture.spawn(req).await.unwrap();
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
  - { path: outside/scope.rs, content: "lands anyway" }
outcome: { type: completed, summary: "done" }
"#,
    )
    .unwrap();

    let mut req = request(dir.path().to_path_buf());
    req.edit_constraints = Some(vec!["src/**".to_string()]);
    let session = fixture.spawn(req).await.unwrap();
    let _ = drain(session).await;

    // Without the capability, the adapter ignores the constraint
    // instead of failing — the engine's own post-check scope diff
    // is what would catch this later.
    assert!(dir.path().join("outside/scope.rs").exists());
}

#[tokio::test]
async fn probe_reports_healthy() {
    let fixture = MockAdapter::from_yaml("outcome: { type: completed, summary: ok }").unwrap();
    let report = fixture.probe().await.unwrap();
    assert!(matches!(report, ProbeReport::Healthy { .. }), "{report:?}");
}

#[tokio::test]
async fn resume_serves_the_next_script_under_the_same_session_id() {
    // The mock's resume is scripted like spawn, but the stream
    // reports the identity being continued — and records the ask, so
    // engine tests can prove the right conversation was picked up.
    let fixture = MockAdapter::from_yaml("outcome: { type: completed, summary: ok }").unwrap();
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

#[tokio::test]
async fn a_multi_session_fixture_scripts_each_spawn_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter::from_yaml(
        r#"
sessions:
  - outcome: { type: completed, summary: "first session" }
  - outcome: { type: failed, message: "second session", retryable: false }
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
        AgentEvent::Failed { error, retryable: false } if error.message == "second session"
    ));
}

#[tokio::test]
async fn an_exhausted_fixture_refuses_further_spawns_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter::from_yaml(
        r#"
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
sessions:
  - effects: [{ path: first.txt, content: "1" }]
    outcome: { type: completed, summary: "one" }
  - effects: [{ path: second.txt, content: "2" }]
    outcome: { type: completed, summary: "two" }
"#,
    )
    .unwrap();

    let _ = adapter
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    assert!(dir.path().join("first.txt").exists());
    assert!(!dir.path().join("second.txt").exists());

    let _ = adapter
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    assert!(dir.path().join("second.txt").exists());
}

#[tokio::test]
async fn a_single_session_fixture_still_scripts_exactly_one_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter::from_yaml("outcome: { type: completed, summary: ok }").unwrap();

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
    let session_a = adapter
        .spawn(prompt_request(dir_a.path().to_path_buf(), "do task-a now"))
        .await
        .unwrap();
    drain(session_a).await;
    let session_b = adapter
        .spawn(prompt_request(dir_b.path().to_path_buf(), "do task-b now"))
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
    // Fixtures that never set match_prompt_contains keep today's exact
    // behavior — this field is additive, not a breaking change.
    let dir = tempfile::tempdir().unwrap();
    let fixture = r#"
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
            .spawn(request(dir.path().to_path_buf()))
            .await
            .unwrap(),
    )
    .await;
    assert!(dir.path().join("first.txt").exists());
    drain(
        adapter
            .spawn(request(dir.path().to_path_buf()))
            .await
            .unwrap(),
    )
    .await;
    assert!(dir.path().join("second.txt").exists());
}

#[tokio::test]
async fn polling_a_gate_the_forge_never_published_is_a_typed_error() {
    let forge = MockForge::new(MockForgeState::new());
    let gate = PublishedGate {
        url: "https://forge.example/pr/99".to_string(),
        number: 99,
    };
    let error = forge.poll(&gate).await.unwrap_err();
    assert!(
        matches!(error, ForgeError::UnknownGate { number: 99 }),
        "{error:?}"
    );
}

fn gate_request(run_id: &str) -> PublishRequest {
    PublishRequest {
        branch: format!("yunta/{run_id}/gate"),
        base_branch: "main".to_string(),
        run_id: run_id.to_string(),
        summary: "spec ready for review".to_string(),
        artifacts: Vec::new(),
    }
}

#[tokio::test]
async fn the_mock_forge_reuses_only_open_prs() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone());

    let first = forge.publish(&gate_request("run-1")).await.unwrap();
    let again = forge.publish(&gate_request("run-1")).await.unwrap();
    assert_eq!(
        again.number, first.number,
        "an open PR for the run is reused"
    );

    // A person closed it: the gate publishes a new one, never the closed
    // one — the same rule the GitHub forge follows.
    state.close("run-1");
    let reopened = forge.publish(&gate_request("run-1")).await.unwrap();
    assert_ne!(reopened.number, first.number);
    assert_eq!(state.pr_number("run-1"), Some(reopened.number));
}

#[tokio::test]
async fn the_mock_forge_reports_a_merged_pr() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone());
    let published = forge.publish(&gate_request("run-1")).await.unwrap();

    let merge_sha = state.merge("run-1", &"octocat".into());

    let polled = forge.poll(&published).await.unwrap();
    assert_eq!(
        polled.review,
        ReviewOutcome::Merged {
            by: "octocat".into(),
            merge_sha,
        }
    );
}

// --- a mock that proves what it says it proves -------------------------

#[tokio::test]
async fn interrupt_and_kill_are_observably_different() {
    let dir = tempfile::tempdir().unwrap();
    // One scripted session per adapter: each stop is tried on a fresh one.
    let stubborn =
        || MockAdapter::from_yaml("outcome: { type: hang, on_interrupt: ignore }").unwrap();

    // An ordered stop the session ignores: the stream stays open.
    let mut session = stubborn()
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    session.interrupt().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(200), drain(session))
            .await
            .is_err(),
        "a session that ignores interrupt keeps its stream open"
    );

    // A forced stop ends it regardless.
    let mut session = stubborn()
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    session.kill().await.unwrap();
    let events = tokio::time::timeout(Duration::from_millis(200), drain(session))
        .await
        .expect("kill ends the stream promptly");
    assert!(matches!(
        events.as_slice(),
        [AgentEvent::SessionOpened { .. }]
    ));

    // A session that honors the ordered stop ends on interrupt alone.
    let obedient = MockAdapter::from_yaml("outcome: { type: hang }").unwrap();
    let mut session = obedient
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    session.interrupt().await.unwrap();
    let events = tokio::time::timeout(Duration::from_millis(200), drain(session))
        .await
        .expect("interrupt ends a session that honors it");
    assert!(matches!(
        events.as_slice(),
        [AgentEvent::SessionOpened { .. }]
    ));
}

#[tokio::test]
async fn an_ambiguous_prompt_match_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter::from_yaml(
        r#"
sessions:
  - match_prompt_contains: review
    outcome: { type: completed, summary: "first" }
  - match_prompt_contains: review this
    outcome: { type: completed, summary: "second" }
"#,
    )
    .unwrap();

    let error = adapter
        .spawn(prompt_request(
            dir.path().to_path_buf(),
            "please review this",
        ))
        .await
        .err()
        .expect("two differently named scripts claiming one prompt is a fixture error");
    let message = error.to_string();
    assert!(
        message.contains("review this") && message.contains('2'),
        "the error names the needles and how many scripts match: {message}"
    );
}

#[tokio::test]
async fn scripts_sharing_one_needle_serve_a_request_s_attempts_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter::from_yaml(
        r#"
sessions:
  - match_prompt_contains: review
    outcome: { type: completed, summary: "first" }
  - match_prompt_contains: review
    outcome: { type: completed, summary: "second" }
"#,
    )
    .unwrap();

    for expected in ["first", "second"] {
        let events = drain(
            adapter
                .spawn(prompt_request(dir.path().to_path_buf(), "please review"))
                .await
                .unwrap(),
        )
        .await;
        assert!(
            matches!(events.last(), Some(AgentEvent::Completed { result }) if result.summary == expected),
            "got: {events:?}"
        );
    }
}

#[tokio::test]
async fn session_ids_count_per_adapter_not_per_process() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = "outcome: { type: completed, summary: \"done\" }";
    for _ in 0..2 {
        let adapter = MockAdapter::from_yaml(fixture).unwrap();
        let events = drain(
            adapter
                .spawn(request(dir.path().to_path_buf()))
                .await
                .unwrap(),
        )
        .await;
        assert!(
            matches!(
                &events[0],
                AgentEvent::SessionOpened { session_id, .. } if session_id.as_str() == "mock-session-1"
            ),
            "every adapter numbers its sessions from one: {events:?}"
        );
    }
}

#[test]
fn a_script_naming_an_agent_is_refused() {
    let error =
        MockAdapter::from_yaml("agent: benito\noutcome: { type: completed, summary: \"done\" }")
            .err()
            .expect("`agent` is not a script's field: the request names the agent");
    assert!(error.to_string().contains("agent"), "{error}");
}

#[tokio::test]
async fn unconsumed_names_the_scripts_no_spawn_claimed() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter::from_yaml(
        r#"
sessions:
  - outcome: { type: completed, summary: "first" }
  - outcome: { type: completed, summary: "second" }
  - outcome: { type: completed, summary: "third" }
"#,
    )
    .unwrap();
    assert_eq!(adapter.unconsumed(), vec![0, 1, 2]);

    drain(
        adapter
            .spawn(request(dir.path().to_path_buf()))
            .await
            .unwrap(),
    )
    .await;

    assert_eq!(
        adapter.unconsumed(),
        vec![1, 2],
        "a run that opened one session leaves the other two scripts unclaimed"
    );
}
