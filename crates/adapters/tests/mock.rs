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
use yunta_core::rail::{RailAct, RailAnswer, RailCall, RailEndpoint};

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use futures::StreamExt;
use yunta_adapters::{
    ClaudeCodeAdapter, MockAdapter, MockFixture, MockForge, MockForgeState, RunPaths,
};
use yunta_core::events::{FailureCause, RefusalCause};
use yunta_core::port::{
    Adapter, AgentEvent, Forge, ForgeError, GateDecision, PermissionProfile, ProbeReport,
    PublishRequest, PullRequestRef, ReviewOutcome, RunToolsEndpoint, SessionRequest,
};
use yunta_core::{
    AdapterSettings, Capabilities, Capability, Confinement, EditSandbox, FenceLevel, RunTool,
    Secret, SessionId, ToolNaming,
};
use yunta_testkit_core::adapter::{confine, drain, request};

#[path = "mock/outcomes.rs"]
mod outcomes;

#[path = "mock/confinement.rs"]
mod confinement;

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

/// A `/rail` that rules as the engine's rail does: a write is judged by the
/// judge's fence under its confinement, and each path it refuses is answered
/// with the fence's own refusal, named as the adapter names tools.
async fn judging_rail(judge: Judge) -> RailEndpoint {
    let app = Router::new()
        .route("/rail/{moment}", post(rule))
        .with_state(judge);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    RailEndpoint {
        url: format!("http://{addr}/rail"),
        token: Secret::new("judge-token".to_string()),
    }
}

/// What a `/rail` judge decides by, and the names its refusals use.
#[derive(Clone)]
struct Judge {
    fence: Fence,
    worktree: PathBuf,
    confinement: Confinement,
    naming: ToolNaming,
}

/// The engine's answer to one hook call: a write is refused when the fence
/// refuses any path it names, judged under the confinement; every other
/// moment goes on.
async fn rule(State(judge): State<Judge>, Json(call): Json<RailCall>) -> Json<RailAnswer> {
    let RailAct::BeforeWrite { paths } = call.act else {
        return Json(RailAnswer::Proceed);
    };
    let reasons: Vec<String> = paths
        .iter()
        .filter_map(
            |path| match judge.fence.judge(&judge.worktree, path, &judge.confinement) {
                Verdict::Refused(refusal) => Some(refusal.message(judge.naming)),
                Verdict::Allowed => None,
            },
        )
        .collect();
    Json(if reasons.is_empty() {
        RailAnswer::Proceed
    } else {
        RailAnswer::Refuse {
            reason: reasons.join("\n\n"),
        }
    })
}

/// The request with its hooks pointed at a judge of its own fence, which names
/// tools as `adapter` does. A session scripted to write asks before each
/// write, as a CLI's write hook does, and a write the fence refuses is not made.
async fn judged(mut req: SessionRequest, adapter: &MockAdapter) -> SessionRequest {
    let judge = Judge {
        fence: req.fence.clone(),
        worktree: req.cwd.clone(),
        confinement: req.confinement.clone(),
        naming: adapter.capabilities().tool_naming,
    };
    req.rail = Some(judging_rail(judge).await);
    req
}

/// The session asks `/rail` before each write, and that judge applies the
/// fence's own function as the engine's rail does, so the fixture exercises
/// the rule rather than a second implementation of it.
#[tokio::test]
async fn a_fixture_effect_outside_the_fence_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        r#"
dialect: claude-code
effects:
  - { path: src/lib.rs, content: "pub fn hello() {}\n" }
  - { path: outside/scope.rs, content: "should never land" }
outcome: { type: completed, summary: "done" }
"#,
    )
    .unwrap();

    let mut req = request(dir.path().to_path_buf());
    req.fence = Fence {
        allowed: Some(vec!["src/**".into()]),
        advice: Advice::ReportFinding,
    };
    confine(
        &mut req,
        PermissionProfile::Edit,
        EditSandbox::default(),
        true,
        true,
    )
    .unwrap();
    let session = fixture.spawn(judged(req, &fixture).await).await.unwrap();
    drain(session).await;

    assert!(dir.path().join("src/lib.rs").exists());
    assert!(!dir.path().join("outside/scope.rs").exists());
}

/// The same refusal under the codex dialect, whose hook refuses a patch only
/// when its reply gives the reason on stderr: a reply that loses the reason
/// lets the write through, as it would under the CLI.
#[tokio::test]
async fn a_codex_fixture_effect_outside_the_fence_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        r#"
dialect: codex
effects:
  - { path: src/lib.rs, content: "pub fn hello() {}\n" }
  - { path: outside/scope.rs, content: "should never land" }
outcome: { type: completed, summary: "done" }
"#,
    )
    .unwrap();

    let mut req = request(dir.path().to_path_buf());
    req.fence = Fence {
        allowed: Some(vec!["src/**".into()]),
        advice: Advice::ReportFinding,
    };
    confine(
        &mut req,
        PermissionProfile::Edit,
        EditSandbox::default(),
        true,
        true,
    )
    .unwrap();
    let session = fixture.spawn(judged(req, &fixture).await).await.unwrap();
    drain(session).await;

    assert!(dir.path().join("src/lib.rs").exists());
    assert!(!dir.path().join("outside/scope.rs").exists());
}

#[tokio::test]
async fn mock_refuses_a_tool_call_fence_it_cannot_build() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::from_yaml(
        "dialect: claude-code\nsessions:\n  - outcome: {type: completed, summary: ok}",
    )
    .unwrap();
    let mut req = request(dir.path().to_path_buf());
    req.rail = None;
    assert!(
        mock.spawn(req).await.is_err(),
        "a tool-call fence without a hook opens nothing"
    );
}

/// A filesystem sandbox keeps a write out without a word, as a CLI's own
/// sandbox does: the effect is left unwritten and unreported, and the
/// session goes on to its outcome.
#[tokio::test]
async fn a_filesystem_fence_leaves_the_denied_effect_unwritten_and_unreported() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let denied = outside.path().join("scope.rs");
    let fixture = MockAdapter::from_yaml(&format!(
        r#"
dialect: claude-code
effects:
  - {{ path: "{}", content: "should never land" }}
outcome: {{ type: completed, summary: "done" }}
"#,
        denied.display()
    ))
    .unwrap()
    .narrowed(Capabilities {
        fence: FenceLevel::Filesystem,
        ..Capabilities::default()
    });

    let session = fixture
        .spawn(request(dir.path().to_path_buf()))
        .await
        .unwrap();
    let events = drain(session).await;

    assert!(!denied.exists(), "the denied write landed: {denied:?}");
    assert!(
        events.iter().all(|e| matches!(
            e,
            AgentEvent::SessionOpened { .. } | AgentEvent::Completed { .. }
        )),
        "the sandbox reports nothing for the write it kept out: {events:?}"
    );
    assert!(matches!(
        events.last().unwrap(),
        AgentEvent::Completed { result } if result.summary == "done"
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
    // Fixtures that never set match_prompt_contains keep today's exact
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

#[tokio::test]
async fn polling_a_gate_the_forge_never_published_is_a_typed_error() {
    let forge = MockForge::new(MockForgeState::new());
    let gate = PullRequestRef {
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
        run_id: run_id.parse().unwrap(),
        decision: GateDecision {
            node: "review".into(),
            question: "Is the spec ready?".to_string(),
            assignee: "lead".to_string(),
            then: Vec::new(),
            corrected_by: None,
        },
        artifacts: Vec::new(),
        shown: Vec::new(),
    }
}

#[tokio::test]
async fn the_mock_forge_moves_the_head_only_when_what_it_publishes_changes() {
    let state = MockForgeState::new();
    let forge = MockForge::new(state.clone());
    let mut request = gate_request("run-1");
    request.artifacts = vec![("spec.md".to_string(), b"# spec".to_vec())];

    let published = forge.publish(&request).await.unwrap();
    let first = forge.poll(&published).await.unwrap().head_sha;
    forge.publish(&request).await.unwrap();
    assert_eq!(
        forge.poll(&published).await.unwrap().head_sha,
        first,
        "the same files commit nothing"
    );

    request.artifacts = vec![("spec.md".to_string(), b"# spec, corrected".to_vec())];
    forge.publish(&request).await.unwrap();
    assert_ne!(forge.poll(&published).await.unwrap().head_sha, first);
    let pr = state.pull_requests().remove(0);
    assert_eq!(pr.files["spec.md"], b"# spec, corrected");
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

/// One scripted session per call, which ignores an ordered stop: each stop
/// the test tries is tried on a fresh one.
fn stubborn() -> MockAdapter {
    MockAdapter::from_yaml("dialect: claude-code\noutcome: { type: hang, on_interrupt: ignore }")
        .unwrap()
}

#[tokio::test]
async fn interrupt_and_kill_are_observably_different() {
    let dir = tempfile::tempdir().unwrap();

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
    let obedient = MockAdapter::from_yaml("dialect: claude-code\noutcome: { type: hang }").unwrap();
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
dialect: claude-code
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
dialect: claude-code
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
    let fixture = "dialect: claude-code\noutcome: { type: completed, summary: \"done\" }";
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
    let error = MockAdapter::from_yaml(
        "dialect: claude-code\nagent: benito\noutcome: { type: completed, summary: \"done\" }",
    )
    .err()
    .expect("`agent` is not a script's field: the request names the agent");
    assert!(error.to_string().contains("agent"), "{error}");
}

#[tokio::test]
async fn unconsumed_names_the_scripts_no_spawn_claimed() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter::from_yaml(
        r#"
dialect: claude-code
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

#[tokio::test]
async fn a_dropped_session_stops_its_player() {
    // One step scheduled far enough ahead that the player is still
    // waiting for it when the session goes away.
    let adapter = MockAdapter::from_yaml(
        "dialect: claude-code\nsessions:\n  - steps:\n      - { type: note, text: later, after_ms: 3600000 }\n    \
         outcome: { type: completed, summary: done }\n",
    )
    .unwrap();
    let alive = || {
        tokio::runtime::Handle::current()
            .metrics()
            .num_alive_tasks()
    };

    let before = alive();
    let mut session = adapter.spawn(request(std::env::temp_dir())).await.unwrap();
    {
        // Reading the opening event proves the player is past it and
        // into the step it has to wait for.
        let mut events = session.events();
        assert!(matches!(
            events.next().await,
            Some(AgentEvent::SessionOpened { .. })
        ));
    }
    assert!(
        alive() > before,
        "the session's script is played by a task of its own",
    );

    drop(session);
    // Cancellation lands when the runtime next drives the task, so the
    // test hands it the turns rather than waiting a wall-clock interval.
    for _ in 0..1_000 {
        if alive() <= before {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("a dropped session leaves its player waiting on a step nobody will read");
}

#[tokio::test]
async fn a_mock_session_stages_the_skill_its_cli_would() {
    let dir = tempfile::tempdir().unwrap();
    let (skill, cwd) = skill_and_cwd(dir.path());
    let mock = MockAdapter::from_yaml(
        "dialect: claude-code\nsessions:\n  - outcome: {type: completed, summary: ok}",
    )
    .unwrap();
    let mut req = request(cwd);
    req.skills = vec![skill];
    let staged = mock.staged_paths(&req);
    assert!(
        !staged.is_empty(),
        "the mock declares the mount its CLI would"
    );
    let link = req.cwd.join(&staged[0]);
    let session = mock.spawn(req).await.unwrap();
    assert!(
        link.symlink_metadata().is_ok(),
        "linked while the session lives"
    );
    drop(session);
    assert!(link.symlink_metadata().is_err(), "taken back when it ends");
}

/// A skill source and a worktree, both created under `dir`.
fn skill_and_cwd(dir: &Path) -> (PathBuf, PathBuf) {
    let (skill, cwd) = (dir.join("skills-src/grill"), dir.join("worktree"));
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    (skill, cwd)
}

#[tokio::test]
async fn a_mock_run_tool_call_ends_within_the_call_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::from_yaml(
        "dialect: claude-code\nsessions:\n  - steps: [{type: run_tool, tool: yunta_check_artifact}]\n    outcome: {type: completed, summary: ok}",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _held = listener.accept().await;
        std::future::pending::<()>().await;
    });
    let mut req = request(dir.path().to_path_buf());
    req.budget.timeout = Some(Duration::from_millis(300));
    req.run_tools_endpoint = Some(RunToolsEndpoint {
        url: format!("http://{addr}/mcp"),
        token: Secret::new("token".to_string()),
    });
    let events = tokio::time::timeout(
        Duration::from_secs(20),
        drain(mock.spawn(req).await.unwrap()),
    )
    .await
    .expect("ends within the call timeout");
    assert!(
        matches!(events.last(), Some(AgentEvent::Failed { .. })),
        "{events:?}"
    );
}

/// A fixture names the run's own directories — where a session writes
/// the file its node declares, above all — and it has to mean the same
/// thing whichever caller parses it. The `yunta test` harness rendered
/// those names and the run bench did not, so the same YAML scripted a
/// real path from one caller and a literal `{{run.staging}}` from the other.
#[test]
fn a_fixture_renders_its_run_paths_wherever_it_is_parsed() {
    let yaml = "\
dialect: claude-code
sessions:
  - effects:
      - path: \"{{run.staging}}/grill/brief.md\"
        content: hi
    outcome: { type: completed, summary: done }
";
    let fixture = MockFixture::parse(
        yaml,
        &RunPaths {
            run_dir: Path::new("/runs/r1"),
            worktree: Path::new("/runs/r1/tree"),
            staging: Path::new("/runs/r1/scratch/staging"),
        },
    )
    .expect("the fixture parses");
    assert_eq!(
        fixture.sessions[0].effects[0].path,
        PathBuf::from("/runs/r1/scratch/staging/grill/brief.md"),
        "`{{{{run.staging}}}}` names the directory the run granted the node"
    );
}

/// A caller with no run in hand still gets one answer, not a wrong one:
/// the fixture that names a directory the caller cannot resolve is
/// refused, rather than scripting a session to write to a path spelled
/// `{{run.staging}}`.
#[test]
fn a_fixture_naming_a_run_directory_is_refused_where_there_is_no_run() {
    let yaml = "\
dialect: claude-code
sessions:
  - effects:
      - path: \"{{run.staging}}/grill/brief.md\"
        content: hi
    outcome: { type: completed, summary: done }
";
    let refused = MockFixture::parse_without_a_run(yaml).expect_err("no run defines `staging`");
    assert!(
        refused.to_string().contains("run paths"),
        "the refusal names what could not be resolved: {refused}"
    );
}

/// A fixture declares no capabilities of its own: the mock answers with the
/// set of the CLI its dialect names, flag for flag. A second copy beside the
/// fixture could drift from the adapter's, and a test would then claim a
/// capability the engine never saw.
#[test]
fn every_capability_comes_from_the_dialect() {
    let fixture = MockFixture::parse_without_a_run(
        "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: ok }\n",
    )
    .unwrap();
    let declared = MockAdapter::new(fixture).capabilities();
    let cli = ClaudeCodeAdapter::new(&AdapterSettings::default()).capabilities();
    for capability in Capability::ALL {
        assert_eq!(
            declared.declares(capability),
            cli.declares(capability),
            "`{capability}` is declared as the CLI declares it"
        );
    }
    assert_eq!(
        declared, cli,
        "the whole set, naming included, is the CLI's"
    );
}

/// And a key the fixture does not know is refused, naming it: a fixture is
/// authored, so a typo is a mistake and never a silent default.
#[test]
fn a_fixture_with_an_unknown_top_level_key_is_refused() {
    let error = MockFixture::parse_without_a_run(
        "dialect: claude-code\nteleportation: true\nsessions:\n  - outcome: { type: completed, summary: ok }\n",
    )
    .expect_err("an unknown top-level key is refused");
    assert!(
        error.to_string().contains("teleportation"),
        "the refusal names the key: {error}"
    );
}

/// A bare protocol name is not a call a CLI makes: a run tool is named the
/// way the dialect's CLI names it, and a step that writes the bare name is
/// refused when the fixture is read.
#[test]
fn a_mock_names_run_tools_the_way_its_cli_does() {
    let mock = MockAdapter::from_yaml(
        "dialect: claude-code\nsessions:\n  - outcome: {type: completed, summary: ok}",
    )
    .unwrap();
    assert_ne!(
        mock.capabilities().tool_naming,
        ToolNaming::Bare,
        "a run-tool fixture names tools the way its CLI does"
    );
    let bare = "dialect: claude-code\nsessions:\n  - steps: [{type: tool_use, name: yunta_task, target: x}]\n    outcome: {type: completed, summary: ok}";
    assert!(
        MockFixture::parse_without_a_run(bare).is_err(),
        "a bare run-tool name is refused on a prefixed CLI"
    );
}
