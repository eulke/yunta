//! The `mock` adapter and the forge beside it: a session is whatever the
//! fixture scripted for it, and nothing else.
//!
//! Every behavior a real adapter is asked for is exercised here against
//! a script — the terminal event a session always pays, the effects the
//! fence lets through, resume under one session id, the capabilities the
//! fixture declares, and a gate published to a pull request — so the
//! engine's own suites can trust the adapter they run on.

use std::path::{Path, PathBuf};
use std::time::Duration;
use yunta_core::fence::{Advice, Fence};

use futures::StreamExt;
use yunta_adapters::{MockAdapter, MockFixture, MockForge, MockForgeState, RunPaths};
use yunta_core::port::{
    Adapter, AgentEvent, Forge, ForgeError, ProbeReport, PublishRequest, PublishedGate,
    ReviewOutcome, SessionRequest,
};
use yunta_core::{Capabilities, SessionId};
use yunta_testkit_core::adapter::{drain, request};

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

/// The mock judges by the same function every real adapter asks, so a
/// fixture exercises the rule rather than a second implementation of it.
#[tokio::test]
async fn a_fixture_effect_outside_the_fence_is_refused_and_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        r#"
capabilities: { fence: tool_calls }
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
        roots: Vec::new(),
        advice: Advice::ReportFinding,
    };
    let session = fixture.spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(dir.path().join("src/lib.rs").exists());
    assert!(!dir.path().join("outside/scope.rs").exists());

    // The refused path reaches the log as the path it is: a path names
    // the repository, which is what a reader needs to see.
    let refused = events.iter().any(|e| {
        matches!(e, AgentEvent::WriteRefused { target }
            if target.display.as_deref() == Some("outside/scope.rs"))
    });
    assert!(
        refused,
        "the refused write is recorded, naming the path: {events:?}"
    );
}

#[tokio::test]
async fn without_a_fence_the_adapter_builds_none_and_the_effect_lands() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        r#"
capabilities: { fence: none }
effects:
  - { path: outside/scope.rs, content: "lands anyway" }
outcome: { type: completed, summary: "done" }
"#,
    )
    .unwrap();

    let mut req = request(dir.path().to_path_buf());
    req.fence = Fence {
        allowed: Some(vec!["src/**".into()]),
        roots: Vec::new(),
        advice: Advice::ReportFinding,
    };
    let session = fixture.spawn(req).await.unwrap();
    let _ = drain(session).await;

    // With no fence to build, the adapter writes what it scripted —
    // the engine's own post-check scope diff is what catches it later.
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
    assert_eq!(caps.fence, yunta_core::FenceLevel::None);
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

#[tokio::test]
async fn a_dropped_session_stops_its_player() {
    // One step scheduled far enough ahead that the player is still
    // waiting for it when the session goes away.
    let adapter = MockAdapter::from_yaml(
        "sessions:\n  - steps:\n      - { type: note, text: later, after_ms: 3600000 }\n    \
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

/// A fixture names the run's own directories — where a session writes
/// the file its node declares, above all — and it has to mean the same
/// thing whichever caller parses it. The `yunta test` harness rendered
/// those names and the run bench did not, so the same YAML scripted a
/// real path from one caller and a literal `{{run.staging}}` from the other.
#[test]
fn a_fixture_renders_its_run_paths_wherever_it_is_parsed() {
    let yaml = "\
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

/// The fixture's capability twin names the same eight fields the port
/// does, and each one a fixture declares reaches the adapter. A twin
/// that drifted would let a test claim a capability the engine never
/// saw — or hide one it did.
#[test]
fn every_capability_round_trips_through_a_fixture() {
    for capability in yunta_core::Capability::ALL {
        // The fence is a level, not a flag: a fixture names which of
        // the three it builds, and the other seven stay booleans.
        let declares = match capability {
            yunta_core::Capability::Fence => "tool_calls",
            _ => "true",
        };
        let fixture = MockFixture::parse_without_a_run(&format!(
            "capabilities: {{ {}: {declares} }}\nsessions:\n  - outcome: {{ type: completed, summary: ok }}\n",
            capability.as_str()
        ))
        .unwrap_or_else(|e| panic!("a fixture declaring `{capability}` parses: {e}"));
        let declared = MockAdapter::new(fixture).capabilities();
        for other in yunta_core::Capability::ALL {
            assert_eq!(
                declared.declares(other),
                other == capability,
                "a fixture declaring `{capability}` declares it and nothing else"
            );
        }
    }
}

/// And a flag the twin does not know is refused, naming it: a fixture is
/// authored, so a typo is a mistake and never a silent `false`.
#[test]
fn a_fixture_that_declares_an_unknown_capability_is_refused() {
    let error = MockFixture::parse_without_a_run(
        "capabilities: { teleportation: true }\nsessions:\n  - outcome: { type: completed, summary: ok }\n",
    )
    .expect_err("an unknown capability flag is refused");
    assert!(
        error.to_string().contains("teleportation"),
        "the refusal names the flag: {error}"
    );
}
