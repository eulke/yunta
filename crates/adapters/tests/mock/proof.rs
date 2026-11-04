//! Tests that the mock adapter does what it says: an interrupt and a kill end a
//! session differently, an ambiguous match is an error, and a script's claims
//! hold when it runs.

use super::*;

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
