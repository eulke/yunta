//! The per-run MCP listener, exercised by a
//! real rmcp client over real loopback HTTP — token auth, the four
//! tools, and the scoping rules as observable behavior,
//! not as unit assertions on internals.

use std::sync::Arc;

use rmcp::model::CallToolRequestParams;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ServiceExt;
use serde_json::json;
use yunta_core::events::{
    CriteriaCheckedPayload, CriterionResult, CriterionType, EventPayload, FindingEvent, NodeEvent,
    Phase, ScopeCheckedPayload, TaskEvent, TaskStatus, TaskStatusChangedPayload,
};
use yunta_core::TaskId;
use yunta_engine::RunToolsSession;
use yunta_testkit::ToolsHost;

const BLACKBOARD_WORKFLOW: &str = r#"
name: coordinated
nodes:
  - id: solo
    kind: bash
    run: "true"
  - id: cooperative
    kind: parallel
    coordination: blackboard
    nodes:
      - id: worker-a
        kind: bash
        run: "true"
      - id: worker-b
        kind: bash
        run: "true"
  - id: evaluative
    kind: parallel
    nodes:
      - id: reviewer
        kind: bash
        run: "true"
"#;

/// Puts a finding under `node` on the log, as a node that already
/// reported one leaves it.
fn seed_finding(host: &ToolsHost, node: &str, id: &str) {
    host.record(
        Some(node),
        EventPayload::Findings(FindingEvent::Posted(
            yunta_core::events::FindingPostedPayload {
                finding: yunta_core::events::Finding {
                    id: id.into(),
                    severity: yunta_core::events::FindingSeverity::Minor,
                    title: format!("seeded {id}"),
                    location: "src/lib.rs".into(),
                    detail: "seeded directly".to_string(),
                    proposed_criterion: None,
                },
            },
        )),
    );
}

/// The id of every finding posted under `node`, in the order the log
/// carries them.
fn findings_by(host: &ToolsHost, node: &str) -> Vec<String> {
    host.events()
        .into_iter()
        .filter(|e| e.node_id.as_ref().map(|n| n.as_str()) == Some(node))
        .filter_map(|e| match e.payload() {
            Some(EventPayload::Findings(FindingEvent::Posted(p))) => Some(p.finding.id.to_string()),
            _ => None,
        })
        .collect()
}

async fn client_for(
    session: &RunToolsSession,
    token_override: Option<&str>,
) -> Result<
    rmcp::service::RunningService<rmcp::RoleClient, ()>,
    Box<rmcp::service::ClientInitializeError>,
> {
    let token = token_override.unwrap_or(session.endpoint.token.expose());
    // rmcp prepends `Bearer ` itself — pass the bare token.
    let config = StreamableHttpClientTransportConfig::with_uri(session.endpoint.url.clone())
        .auth_header(token.to_string());
    let transport = StreamableHttpClientTransport::with_client(reqwest::Client::default(), config);
    ().serve(transport).await.map_err(Box::new)
}

fn text_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|b| b.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

async fn call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    tool: &str,
    args: serde_json::Value,
) -> (bool, String) {
    let mut params = CallToolRequestParams::new(tool.to_string());
    if let Some(object) = args.as_object() {
        if !object.is_empty() {
            params = params.with_arguments(object.clone());
        }
    }
    let result = client.call_tool(params).await.unwrap();
    (result.is_error.unwrap_or(false), text_of(&result))
}

// --- Auth (single-use bearer token) --------------------------------

#[tokio::test]
async fn a_wrong_bearer_token_is_rejected_before_any_tool_runs() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host.session("solo", None).await;

    let denied = client_for(&session, Some("not-the-token")).await;
    assert!(
        denied.is_err(),
        "a client with the wrong token must not even initialize"
    );

    let allowed = client_for(&session, None).await.unwrap();
    let tools = allowed.list_tools(None).await.unwrap();
    assert!(!tools.tools.is_empty());
    allowed.cancel().await.unwrap();
}

// --- yunta_post_finding (one schema both ways) -------------------------

#[tokio::test]
async fn post_finding_lands_on_the_log_under_the_sessions_own_node() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host.session("solo", None).await;
    let client = client_for(&session, None).await.unwrap();

    let (is_error, text) = call(
        &client,
        "yunta_post_finding",
        json!({
            "id": "hot-1",
            "severity": "major",
            "title": "found mid-work",
            "location": "src/auth.rs:42",
            "detail": "the token check is bypassable",
        }),
    )
    .await;
    assert!(!is_error, "got: {text}");
    assert_eq!(findings_by(&host, "solo"), vec!["hot-1"]);

    // An incomplete report is a visible error naming the schema — and a
    // no-op on the log: reporting badly must fail visibly, not silently.
    let (is_error, text) = call(
        &client,
        "yunta_post_finding",
        json!({"id": "bad", "title": "no severity"}),
    )
    .await;
    assert!(is_error);
    assert!(text.contains("severity"), "got: {text}");
    assert_eq!(findings_by(&host, "solo").len(), 1);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_tool_written_event_carries_the_runs_injected_clock() {
    // The finding a tool writes is stamped by the run's own clock, not a
    // fresh `SystemClock` — so a run driven by a fixed clock is
    // reproducible through its listener's events too.
    let clock = Arc::new(yunta_testkit_core::AtClock::rfc3339("2020-02-02T02:02:02Z"));
    let host = ToolsHost::stamped_by(BLACKBOARD_WORKFLOW, clock.clone());
    let session = host.session("solo", None).await;
    let client = client_for(&session, None).await.unwrap();

    let (is_error, text) = call(
        &client,
        "yunta_post_finding",
        json!({
            "id": "clocked",
            "severity": "note",
            "title": "stamped by the injected clock",
            "location": "src/lib.rs:1",
            "detail": "the timestamp is the run's, not wall time",
        }),
    )
    .await;
    assert!(!is_error, "got: {text}");
    client.cancel().await.unwrap();

    let event = host
        .events()
        .into_iter()
        .find(|e| {
            matches!(
                e.payload(),
                Some(EventPayload::Findings(FindingEvent::Posted(p))) if p.finding.id.as_str() == "clocked"
            )
        })
        .expect("the finding is on the log");
    assert_eq!(event.timestamp, yunta_core::Clock::now(clock.as_ref()));
}

// --- yunta_get_blackboard (mount rule, read rule) --------------------

#[tokio::test]
async fn blackboard_is_not_mounted_outside_a_blackboard_group() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    // `solo` is no group's child; `reviewer` sits in an `independent`
    // group — neither may even see the tool.
    for node in ["solo", "reviewer"] {
        let session = host.session(node, None).await;
        let client = client_for(&session, None).await.unwrap();
        let tools = client.list_tools(None).await.unwrap();
        assert!(
            !tools.tools.iter().any(|t| t.name == "yunta_get_blackboard"),
            "node `{node}` must not see the blackboard tool"
        );
        let (is_error, text) = call(&client, "yunta_get_blackboard", json!({})).await;
        assert!(is_error, "node `{node}` calling anyway must be refused");
        assert_eq!(
            text,
            "this session's node is not in a `coordination: blackboard` group — the blackboard is never mounted outside one",
            "node `{node}`"
        );
        client.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn blackboard_serves_own_posts_only_while_the_group_runs() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    // A sibling (worker-b) and a foreign node (reviewer) already posted.
    seed_finding(&host, "worker-b", "sibling-post");
    seed_finding(&host, "reviewer", "foreign-post");

    let session = host.session("worker-a", None).await;
    let client = client_for(&session, None).await.unwrap();

    let (is_error, text) = call(
        &client,
        "yunta_post_finding",
        json!({
            "id": "own-post",
            "severity": "note",
            "title": "mine",
            "location": "src/x.rs",
            "detail": "posted by worker-a",
        }),
    )
    .await;
    assert!(!is_error, "got: {text}");

    // Pre-join, the board shows worker-a its OWN post and nothing
    // else — not the sibling's, not the foreign node's.
    let (is_error, text) = call(&client, "yunta_get_blackboard", json!({})).await;
    assert!(!is_error, "got: {text}");
    let board: serde_json::Value = serde_json::from_str(&text).unwrap();
    let ids: Vec<&str> = board["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["own-post"]);

    // Taken back, it leaves the board its author reads, the same way it
    // leaves the group's consolidation.
    let (is_error, text) = call(
        &client,
        "yunta_withdraw_finding",
        json!({ "id": "own-post", "reason": "it was the harness, not the code" }),
    )
    .await;
    assert!(!is_error, "got: {text}");
    let (is_error, text) = call(&client, "yunta_get_blackboard", json!({})).await;
    assert!(!is_error, "got: {text}");
    let board: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(
        board["findings"].as_array().unwrap().is_empty(),
        "a withdrawn post is not what its author still stands by: {text}",
    );
    client.cancel().await.unwrap();
}

// --- yunta_task_status (read-only tasks view) -------------------------------

#[tokio::test]
async fn task_status_reflects_the_task_state_derived_from_the_log() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    host.record(
        Some("implement"),
        EventPayload::Tasks(TaskEvent::Registered(
            yunta_core::events::TaskRegisteredPayload {
                task_id: TaskId::from("T001"),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            },
        )),
    );

    let session = host.session("solo", None).await;
    let client = client_for(&session, None).await.unwrap();
    let (is_error, text) = call(&client, "yunta_task_status", json!({})).await;
    assert!(!is_error, "got: {text}");
    let status: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(status, json!({ "T001": "pending" }));
    client.cancel().await.unwrap();
}

// --- yunta_request_scope_expansion (task sessions only) ----------------

#[tokio::test]
async fn scope_expansion_request_round_trips_through_the_real_consumer() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host.session("implement", Some("T001")).await;
    let client = client_for(&session, None).await.unwrap();

    let (is_error, text) = call(
        &client,
        "yunta_request_scope_expansion",
        json!({
            "paths": ["src/session/store.rs"],
            "reason": "the auth middleware needs a session store that doesn't exist",
            "proposed_criterion": {"cmd": "cargo test -p auth"},
        }),
    )
    .await;
    assert!(!is_error, "got: {text}");

    // One request per attempt — a second one is refused while
    // the first sits unconsumed.
    let (is_error, text) = call(
        &client,
        "yunta_request_scope_expansion",
        json!({"paths": ["src/other.rs"], "reason": "another"}),
    )
    .await;
    assert!(is_error);
    assert_eq!(
        text,
        "a scope expansion request is already pending for this attempt — one request per attempt"
    );

    // The file the tool wrote is the exact artifact the engine's
    // existing post-attempt evaluation consumes — proven by parsing it
    // with the real consumer, not a mirror type.
    let request = yunta_engine::scope_expansion::load_request(&host.attempt_dir())
        .await
        .unwrap()
        .expect("the request file must exist and parse");
    assert_eq!(request.paths, vec!["src/session/store.rs"]);
    assert_eq!(
        request.proposed_criterion.map(|c| c.cmd),
        Some("cargo test -p auth".to_string())
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn scope_expansion_is_refused_for_sessions_without_a_task() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host.session("solo", None).await;
    let client = client_for(&session, None).await.unwrap();

    let tools = client.list_tools(None).await.unwrap();
    assert!(
        !tools
            .tools
            .iter()
            .any(|t| t.name == "yunta_request_scope_expansion"),
        "a task-less session must not even see the tool"
    );
    let (is_error, text) = call(
        &client,
        "yunta_request_scope_expansion",
        json!({"paths": ["x"], "reason": "y"}),
    )
    .await;
    assert!(is_error);
    assert_eq!(
        text,
        "scope expansion is task machinery, keyed by task — this session has no task; a prompt node's scope is fixed by its own declaration"
    );
    client.cancel().await.unwrap();
}

// --- yunta_task / yunta_check_task (task sessions only) ----------------

/// A task as the tasks document writes it: notes for a runner with no
/// history of the repo, one criterion red until the work is done, and a
/// guard that must stay green.
fn greeting_task() -> yunta_core::Task {
    serde_norway::from_str(
        r#"
id: T001
title: Write the greeting
notes: the greeting lives in hello.txt
scope: [hello.txt]
criteria:
  - cmd: test -f hello.txt
  - { cmd: "true", type: guard }
"#,
    )
    .unwrap()
}

/// A unit that is only a place to stand: a tool that reads never looks
/// at its tree.
fn unit_at(worktree: std::path::PathBuf) -> yunta_engine::Unit {
    yunta_engine::Unit {
        who: yunta_engine::UnitId::Task(TaskId::from("T001")),
        worktree,
        base: yunta_core::CommitSha::from_static("deadbeef"),
        from: yunta_core::TreeId::from_static("deadbeef"),
    }
}

/// A `criteria_checked` of T001, with each command's exit code.
fn checked(phase: Phase, exits: [i32; 2]) -> EventPayload {
    EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
        task_id: TaskId::from("T001"),
        phase,
        results: vec![
            CriterionResult {
                cmd: "test -f hello.txt".to_string(),
                exit_code: exits[0],
                r#type: None,
                reused: false,
                duration_ms: None,
            },
            CriterionResult {
                cmd: "true".to_string(),
                exit_code: exits[1],
                r#type: Some(CriterionType::Guard),
                reused: false,
                duration_ms: None,
            },
        ],
    }))
}

/// A log in the middle of T001's second cycle: a check from the cycle
/// before, the loop setting it running again, this cycle's pre-check,
/// and one attempt that left the criterion red and wrote outside scope.
fn a_second_cycle_with_one_attempt(host: &ToolsHost) {
    let registered = host.record(
        Some("plan"),
        EventPayload::Tasks(TaskEvent::Registered(
            yunta_core::events::TaskRegisteredPayload {
                task_id: TaskId::from("T001"),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            },
        )),
    );
    // A check from a cycle that already ended is not this cycle's.
    host.record(Some("implement"), checked(Phase::Post, [0, 0]));
    host.record(
        Some("implement"),
        EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
            TaskId::from("T001"),
            TaskStatus::Running,
            registered,
        ))),
    );
    host.record(Some("implement"), checked(Phase::Pre, [1, 0]));
    host.record(Some("implement"), checked(Phase::Post, [1, 0]));
    host.record(
        Some("implement"),
        EventPayload::Node(NodeEvent::ScopeChecked(ScopeCheckedPayload {
            task_id: Some(TaskId::from("T001")),
            diff: vec!["notes.md".into()],
            violations: vec!["notes.md".into()],
        })),
    );
}

#[tokio::test]
async fn a_task_session_reads_its_task_and_its_cycle_from_the_run() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    a_second_cycle_with_one_attempt(&host);

    let mut access = host.task_access(greeting_task(), unit_at(host.attempt_dir()));
    access.scope.push("docs/**".into());
    let session = host.task_session("implement", access).await;
    let client = client_for(&session, None).await.unwrap();
    let (is_error, text) = call(&client, "yunta_task", json!({})).await;
    assert!(!is_error, "got: {text}");
    let red = json!({"cmd": "test -f hello.txt", "guard": false, "exit_code": 1});
    let guard = json!({"cmd": "true", "guard": true, "exit_code": 0});
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap(),
        json!({
            "id": "T001",
            "title": "Write the greeting",
            "notes": "the greeting lives in hello.txt",
            "scope": ["hello.txt", "docs/**"],
            "criteria": [
                {"cmd": "test -f hello.txt", "guard": false},
                {"cmd": "true", "guard": true},
            ],
            "checks": [
                {"phase": "pre", "criteria": [red, guard]},
                {"phase": "post", "attempt": 1, "criteria": [red, guard], "outside_scope": ["notes.md"]},
            ],
        }),
        "the task the cycle judges by, its granted scope included, and only this cycle's checks"
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_check_judges_the_work_the_way_its_close_will() {
    let owner = yunta_testkit::Owner::new();
    let repo = tempfile::tempdir().unwrap();
    yunta_testkit::init_repo(repo.path());
    let unit = yunta_engine::Unit {
        from: yunta_engine::head_tree(repo.path(), owner.supervision())
            .await
            .unwrap(),
        base: yunta_engine::head_commit(repo.path(), owner.supervision())
            .await
            .unwrap(),
        ..unit_at(repo.path().to_path_buf())
    };
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let access = host.task_access(greeting_task(), unit);
    let session = host.task_session("implement", access).await;
    let client = client_for(&session, None).await.unwrap();

    // The criterion is still red, and the work strayed outside the scope.
    tokio::fs::write(repo.path().join("notes.md"), "draft")
        .await
        .unwrap();
    let (is_error, text) = call(&client, "yunta_check_task", json!({})).await;
    assert!(!is_error, "got: {text}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap(),
        json!({
            "closes": false,
            "criteria": [
                {"cmd": "test -f hello.txt", "guard": false, "exit_code": 1},
                {"cmd": "true", "guard": true, "exit_code": 0},
            ],
            "outside_scope": ["notes.md"],
        })
    );

    // The work done, and nothing left outside: the close would take it.
    tokio::fs::remove_file(repo.path().join("notes.md"))
        .await
        .unwrap();
    tokio::fs::write(repo.path().join("hello.txt"), "hello")
        .await
        .unwrap();
    let (is_error, text) = call(&client, "yunta_check_task", json!({})).await;
    assert!(!is_error, "got: {text}");
    let verdict: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(verdict["closes"], json!(true), "got: {text}");
    assert_eq!(verdict["outside_scope"], json!([]));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn the_task_tools_are_served_to_task_sessions_only() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);

    let task_session = host.session("implement", Some("T001")).await;
    let client = client_for(&task_session, None).await.unwrap();
    let listed = client.list_tools(None).await.unwrap();
    for tool in ["yunta_task", "yunta_check_task"] {
        assert!(
            listed.tools.iter().any(|t| t.name == tool),
            "a task session is served `{tool}`"
        );
    }
    client.cancel().await.unwrap();

    let session = host.session("solo", None).await;
    let client = client_for(&session, None).await.unwrap();
    let listed = client.list_tools(None).await.unwrap();
    for tool in ["yunta_task", "yunta_check_task"] {
        assert!(
            !listed.tools.iter().any(|t| t.name == tool),
            "a session with no task is not even offered `{tool}`"
        );
    }
    let (is_error, text) = call(&client, "yunta_task", json!({})).await;
    assert!(is_error);
    assert_eq!(
        text,
        "`yunta_task` answers about a task, and this session works none — only a loop's task \
         sessions are served it"
    );
    client.cancel().await.unwrap();
}

// --- ✓ del plan: concurrent posts, no race -----------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_concurrent_sessions_post_interleaved_without_losing_or_misattributing_any() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session_a = host.session("worker-a", None).await;
    let session_b = host.session("worker-b", None).await;
    let client_a = client_for(&session_a, None).await.unwrap();
    let client_b = client_for(&session_b, None).await.unwrap();

    let mut posts = Vec::new();
    for (client, node) in [(&client_a, "worker-a"), (&client_b, "worker-b")] {
        for i in 0..8 {
            posts.push(call(
                client,
                "yunta_post_finding",
                json!({
                    "id": format!("{node}-{i}"),
                    "severity": "note",
                    "title": format!("post {i}"),
                    "location": "src/x.rs",
                    "detail": "concurrent",
                }),
            ));
        }
    }
    for (is_error, text) in futures::future::join_all(posts).await {
        assert!(!is_error, "got: {text}");
    }

    let by_a = findings_by(&host, "worker-a");
    let by_b = findings_by(&host, "worker-b");
    assert_eq!(by_a.len(), 8, "worker-a posts lost: {by_a:?}");
    assert_eq!(by_b.len(), 8, "worker-b posts lost: {by_b:?}");
    assert!(by_a.iter().all(|id| id.starts_with("worker-a-")));
    assert!(by_b.iter().all(|id| id.starts_with("worker-b-")));
    client_a.cancel().await.unwrap();
    client_b.cancel().await.unwrap();
}

// --- the listener dies with its session --------------------------------

#[tokio::test]
async fn dropping_the_session_closes_the_endpoint() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host.session("solo", None).await;
    let url = session.endpoint.url.clone();
    let token = session.endpoint.token.expose().clone();
    drop(session);
    // The graceful shutdown releases the socket on its own schedule.
    yunta_testkit::wait_until_async(
        || {
            let (url, token) = (url.clone(), token.clone());
            async move {
                let config = StreamableHttpClientTransportConfig::with_uri(url).auth_header(token);
                let transport =
                    StreamableHttpClientTransport::with_client(reqwest::Client::default(), config);
                ().serve(transport).await.is_err()
            }
        },
        || "the dropped session's endpoint went on serving clients".to_string(),
    )
    .await;

    let config = StreamableHttpClientTransportConfig::with_uri(url).auth_header(token);
    let transport = StreamableHttpClientTransport::with_client(reqwest::Client::default(), config);
    let attempt = ().serve(transport).await;
    assert!(
        attempt.is_err(),
        "a dropped session's endpoint must be unreachable — credentials never outlive \
         their session"
    );
}

// --- the verdict a session can ask for, before it ends -------------------
//
// The point of the tool is that its answer and the node's close are the
// same code. These name the two halves of that: what it says once the
// document is handed over, and that it says exactly what a failed close
// would while it is not.

fn tasks_spec() -> yunta_core::ArtifactSpec {
    yunta_core::ArtifactSpec::Interpreted(yunta_core::ArtifactKind::Tasks)
}

/// The node those specs belong to, as the close reads it.
fn plan_node() -> yunta_core::Node {
    serde_norway::from_str(
        r#"
id: plan
kind: prompt
prompt: "Write the tasks document."
artifacts:
  produces: [tasks]
"#,
    )
    .unwrap()
}

/// Hands one valid tasks document over, the way the node's own session
/// does.
async fn submit_plan(client: &rmcp::service::RunningService<rmcp::RoleClient, ()>) {
    let (is_error, text) = call(
        client,
        "yunta_submit_tasks",
        json!({
            "name": "plan.yaml",
            "document": {
                "tasks": [{
                    "id": "t1",
                    "title": "Work",
                    "scope": ["src/**"],
                    "criteria": [{"cmd": "cargo test"}],
                }],
            },
        }),
    )
    .await;
    assert!(!is_error, "got: {text}");
}

#[tokio::test]
async fn a_check_reports_what_the_engine_read_not_only_that_it_parsed() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host
        .session_declaring("plan", None, vec![tasks_spec()])
        .await;
    let client = client_for(&session, None).await.unwrap();
    submit_plan(&client).await;

    let (is_error, text) = call(&client, "yunta_check_artifact", json!({})).await;
    assert!(!is_error, "got: {text}");
    assert!(text.contains("tasks — ok"), "{text}");
    assert!(
        text.contains("1 task registered: `t1`"),
        "the session sees its meaning survived, not only its syntax: {text}"
    );
}

#[tokio::test]
async fn a_check_before_the_document_is_handed_over_says_what_the_close_would() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    // A tasks document written by hand where a command node writes one.
    // This node's document arrives through the tool, so the file is not
    // it — and the session is told exactly what its close would say,
    // word for word, instead of a confidence the close will not honour.
    std::fs::write(
        host.staging("plan").join("tasks.yaml"),
        "tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
    )
    .unwrap();
    let session = host
        .session_declaring("plan", None, vec![tasks_spec()])
        .await;
    let client = client_for(&session, None).await.unwrap();
    let (_, text) = call(&client, "yunta_check_artifact", json!({"name": "tasks"})).await;

    let close = yunta_engine::close_artifacts(&plan_node(), &host.run_dir, &[], None)
        .await
        .expect_err("the close owes the document nobody handed over");
    assert!(
        text.contains(&close[0].to_string()),
        "the two answers are one: the check said `{text}`, the close says `{}`",
        close[0]
    );
}

#[tokio::test]
async fn a_check_of_a_submitted_document_reads_the_run_not_the_file_beside_it() {
    // Once a document is handed over it is a fact of the run. The verdict
    // is about that document, so a file somebody wrote over afterwards
    // does not change what the session is told.
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host
        .session_declaring("plan", None, vec![tasks_spec()])
        .await;
    let client = client_for(&session, None).await.unwrap();
    submit_plan(&client).await;

    std::fs::write(
        host.staging("plan").join("tasks.yaml"),
        "not a tasks document at all\n",
    )
    .unwrap();
    let (is_error, text) = call(&client, "yunta_check_artifact", json!({})).await;
    assert!(!is_error, "got: {text}");
    assert!(text.contains("tasks — ok"), "{text}");
    assert!(
        text.contains("1 task registered: `t1`"),
        "the verdict is about the document the run holds: {text}"
    );
}

#[tokio::test]
async fn a_check_of_an_artifact_this_node_never_declared_says_which_it_declares() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host
        .session_declaring("plan", None, vec![tasks_spec()])
        .await;
    let client = client_for(&session, None).await.unwrap();
    let (is_error, text) = call(
        &client,
        "yunta_check_artifact",
        json!({"name": "findings.yaml"}),
    )
    .await;
    assert!(is_error, "{text}");
    assert!(
        text.contains("`findings.yaml` is not an artifact"),
        "{text}"
    );
    assert!(text.contains("`tasks`"), "{text}");
}

/// A node declares the kind, so the tool that submits it is either
/// mounted or absent: there is no argument left for a session to get
/// wrong, and a document this node's close will never look for has no
/// tool to arrive through.
#[tokio::test]
async fn only_the_kinds_this_node_declares_have_a_submission_tool() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host
        .session_declaring("plan", None, vec![tasks_spec()])
        .await;
    let client = client_for(&session, None).await.unwrap();

    let tools = client.list_tools(None).await.unwrap();
    let names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(names.contains(&"yunta_submit_tasks"), "{names:?}");
    assert!(
        !names.contains(&"yunta_submit_questions"),
        "a kind this node does not declare has no way in: {names:?}"
    );

    // And the one that is mounted takes the document alone.
    let schema = &tools
        .tools
        .iter()
        .find(|t| t.name == "yunta_submit_tasks")
        .expect("the tool is mounted")
        .input_schema;
    let properties = schema["properties"].as_object().expect("an object schema");
    assert_eq!(
        properties.keys().collect::<Vec<_>>(),
        vec!["document"],
        "the node declared the kind, so nothing names the artifact: {properties:?}"
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_node_with_nothing_to_check_says_so_rather_than_reporting_success() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host.session("plan", None).await;
    let client = client_for(&session, None).await.unwrap();
    let (is_error, text) = call(&client, "yunta_check_artifact", json!({})).await;
    assert!(is_error, "{text}");
    assert!(text.contains("declares no artifacts"), "{text}");
}

// --- Protocol conformance, at the JSON the wire actually carries -------
//
// These read the result as an agent's own MCP client reads it, over raw
// HTTP: `rmcp`'s client negotiates a revision older than the newest one
// this server announces, so nothing it round-trips can show whether a
// result satisfies that newest revision.

/// The newest revision this server announces, and the one a current
/// coding-agent CLI negotiates.
const MODERN_REVISION: &str = "2026-07-28";

/// A revision of the era before `server/discover`, whose clients open
/// with `initialize` and carry a session id afterwards.
const LEGACY_REVISION: &str = "2025-06-18";

/// The `_meta` a client of [`MODERN_REVISION`] puts on every request:
/// the revision it speaks, who it is, and what it can do. A request
/// without them is not a modern one, and the transport routes it to
/// the session-bearing era instead.
fn modern_meta() -> serde_json::Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": MODERN_REVISION,
        "io.modelcontextprotocol/clientInfo": {"name": "raw-client", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": {},
    })
}

/// One JSON-RPC POST as a client of `revision` sends it: the session's
/// own credential, the two content types the streamable-HTTP transport
/// may answer with, the revision in its header, and the method named in
/// `Mcp-Method` the way SEP-2243 requires of that revision.
async fn post(
    session: &RunToolsSession,
    revision: &str,
    method: &str,
    params: serde_json::Value,
    mcp_session: Option<&str>,
) -> reqwest::Response {
    // A notification carries no id — that is what makes it one, and a
    // server answers it with an acknowledgement rather than a result.
    let mut body = json!({"jsonrpc": "2.0", "method": method, "params": params});
    if !method.starts_with("notifications/") {
        body["id"] = json!(1);
    }
    let mut request = reqwest::Client::new()
        .post(&session.endpoint.url)
        .header(
            "Authorization",
            format!("Bearer {}", session.endpoint.token.expose()),
        )
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .header("MCP-Protocol-Version", revision)
        .header("Mcp-Method", method)
        .body(serde_json::to_string(&body).unwrap());
    if let Some(id) = mcp_session {
        request = request.header("Mcp-Session-Id", id);
    }
    request.send().await.unwrap()
}

/// The JSON-RPC result carried by a response body, whichever shape the
/// transport chose: a bare JSON object, or an event stream whose last
/// non-empty `data:` line is the answer.
fn result_of(body: &str) -> serde_json::Value {
    let message: serde_json::Value = if body.trim_start().starts_with('{') {
        serde_json::from_str(body).unwrap_or_else(|e| panic!("not JSON: {e}\n{body}"))
    } else {
        let data = body
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .unwrap_or_else(|| panic!("no `data:` line in the event stream:\n{body}"));
        serde_json::from_str(data).unwrap_or_else(|e| panic!("not JSON: {e}\n{data}"))
    };
    assert!(
        message.get("error").is_none(),
        "the server refused the request: {message}"
    );
    message["result"].clone()
}

/// Every tool name a list result advertises.
fn tool_names(result: &serde_json::Value) -> Vec<&str> {
    result["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("a list result carries `tools`: {result}"))
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect()
}

/// The tools every session is served, whatever it declares.
fn assert_serves_the_session_tools(result: &serde_json::Value) {
    let names = tool_names(result);
    for expected in [
        "yunta_check_artifact",
        "yunta_post_finding",
        "yunta_update_finding",
        "yunta_withdraw_finding",
        "yunta_task_status",
    ] {
        assert!(
            names.contains(&expected),
            "`{expected}` is missing: {names:?}"
        );
    }
}

#[tokio::test]
async fn a_modern_client_lists_the_session_tools_without_a_handshake() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host.session("solo", None).await;

    let discovered = post(
        &session,
        MODERN_REVISION,
        "server/discover",
        json!({"_meta": modern_meta()}),
        None,
    )
    .await;
    assert_eq!(discovered.status(), 200, "discovery is answered");
    let discovered = result_of(&discovered.text().await.unwrap());
    let announced: Vec<&str> = discovered["supportedVersions"]
        .as_array()
        .unwrap_or_else(|| panic!("discovery announces the revisions served: {discovered}"))
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert!(
        announced.contains(&MODERN_REVISION),
        "the server announces it serves the newest revision: {announced:?}"
    );

    let listed = post(
        &session,
        MODERN_REVISION,
        "tools/list",
        json!({"_meta": modern_meta()}),
        None,
    )
    .await;
    assert_eq!(listed.status(), 200, "the list is answered");
    let listed = result_of(&listed.text().await.unwrap());

    // What the newest revision requires of every list result: a client
    // that validates the shape drops the whole list when one is absent,
    // and the session then has no tool to call.
    assert_eq!(listed["resultType"], "complete", "{listed}");
    assert!(
        listed["ttlMs"].as_u64().is_some(),
        "`ttlMs` is a number the client can cache against: {listed}"
    );
    assert!(
        matches!(listed["cacheScope"].as_str(), Some("public" | "private")),
        "`cacheScope` says who may cache the list: {listed}"
    );
    assert_serves_the_session_tools(&listed);
}

#[tokio::test]
async fn a_legacy_client_still_initializes_and_lists_the_same_tools() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host.session("solo", None).await;

    let opened = post(
        &session,
        LEGACY_REVISION,
        "initialize",
        json!({
            "protocolVersion": LEGACY_REVISION,
            "capabilities": {},
            "clientInfo": {"name": "raw-client", "version": "1"},
        }),
        None,
    )
    .await;
    assert_eq!(opened.status(), 200, "the handshake is answered");
    let mcp_session = opened
        .headers()
        .get("mcp-session-id")
        .expect("the handshake opens a session")
        .to_str()
        .unwrap()
        .to_string();
    let negotiated = result_of(&opened.text().await.unwrap());
    assert_eq!(
        negotiated["protocolVersion"], LEGACY_REVISION,
        "{negotiated}"
    );

    let accepted = post(
        &session,
        LEGACY_REVISION,
        "notifications/initialized",
        json!({}),
        Some(&mcp_session),
    )
    .await;
    assert!(
        accepted.status().is_success(),
        "the notification is accepted: {}",
        accepted.status()
    );

    let listed = post(
        &session,
        LEGACY_REVISION,
        "tools/list",
        json!({}),
        Some(&mcp_session),
    )
    .await;
    assert_eq!(listed.status(), 200, "the list is answered");
    let listed = result_of(&listed.text().await.unwrap());
    assert_serves_the_session_tools(&listed);
}

#[test]
fn the_catalog_and_the_dispatch_name_the_same_tools() {
    // A tool's name is written once. These are the two readings of that
    // one place: the catalog offers a session the name, and the dispatch
    // reads a call back by it — so a name that parses is a name the
    // catalog can offer, and a name the catalog offers is one the
    // dispatch answers.
    for tool in yunta_engine::RunTool::all() {
        assert_eq!(
            yunta_engine::RunTool::parse(tool.name()),
            Some(tool),
            "`{}` is offered and must be answered",
            tool.name()
        );
        assert!(
            tool.name().starts_with("yunta_"),
            "a run tool is named in the engine's own namespace: {}",
            tool.name()
        );
    }
    assert_eq!(yunta_engine::RunTool::parse("yunta_nonesuch"), None);

    // And the set is exactly the submittable kinds plus the nine fixed
    // tools, so a kind that gains a submission tool gains its tool here.
    let submissions = yunta_core::ArtifactKind::ALL
        .into_iter()
        .filter(|kind| kind.submit_tool().is_some())
        .count();
    assert_eq!(yunta_engine::RunTool::all().len(), 9 + submissions);
}
