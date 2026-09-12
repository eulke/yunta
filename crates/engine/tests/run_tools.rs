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
use yunta_core::events::{EventDraft, EventPayload};
use yunta_core::{NodeId, RunId, TaskId, Workflow};
use yunta_engine::{open_session_listener, RunToolsHost, RunToolsSession};
use yunta_storage::Storage;

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

struct Bench {
    _root: tempfile::TempDir,
    run_dir: std::path::PathBuf,
    storage: Storage,
    run_id: RunId,
    host: Arc<RunToolsHost>,
}

/// A clock frozen at a distinctive instant, so a test can prove an event
/// carries the run's injected clock rather than wall time.
struct FrozenClock;

impl yunta_core::Clock for FrozenClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2020-02-02T02:02:02Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }
}

impl Bench {
    fn new() -> Self {
        Self::with_clock(Arc::new(yunta_core::SystemClock))
    }

    fn with_clock(clock: Arc<dyn yunta_core::Clock>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
        let run_id = RunId::from("run-tools-1");
        let workflow: Workflow = serde_norway::from_str(BLACKBOARD_WORKFLOW).unwrap();
        // Every log opens with run_created — the listener's own
        // appends land on an already-born run in production too.
        storage
            .append(
                &EventDraft {
                    run_id: run_id.clone(),
                    node_id: None,
                    payload: EventPayload::RunCreated(yunta_core::events::RunCreatedPayload {
                        manifest_hash: yunta_core::sha256_hex(b"test-manifest"),
                        inputs: Default::default(),
                        mode: "default".into(),
                        promoted_from: None,
                        yunta_schema: None,
                        base_branch: "main".to_string(),
                        base_commit: "deadbeef".into(),
                    }),
                },
                &yunta_core::SystemClock,
            )
            .unwrap();
        let run_dir = root.path().join("run");
        // The directories `create_run` gives every run: what a session
        // writes into, and what the engine writes through.
        for dir in ["artifacts", "scratch"] {
            std::fs::create_dir_all(run_dir.join(dir)).unwrap();
        }
        let host = Arc::new(RunToolsHost::new(
            storage.async_handle(),
            run_id.clone(),
            &workflow,
            clock,
            run_dir.clone(),
            None,
        ));
        Bench {
            _root: root,
            run_dir,
            storage,
            run_id,
            host,
        }
    }

    async fn listener(&self, node: &str, task: Option<&str>) -> RunToolsSession {
        self.listener_for(node, task, Vec::new()).await
    }

    async fn listener_for(
        &self,
        node: &str,
        task: Option<&str>,
        declared: Vec<yunta_core::ArtifactSpec>,
    ) -> RunToolsSession {
        open_session_listener(
            yunta_engine::RunToolsAccess {
                host: self.host.clone(),
                node: NodeId::from(node),
                declared,
            },
            task.map(TaskId::from),
            self._root.path().to_path_buf(),
        )
        .await
        .unwrap()
    }

    fn seed_finding(&self, node: &str, id: &str) {
        self.storage
            .append(
                &EventDraft {
                    run_id: self.run_id.clone(),
                    node_id: Some(NodeId::from(node)),
                    payload: EventPayload::FindingPosted(
                        yunta_core::events::FindingPostedPayload {
                            finding: yunta_core::events::Finding {
                                id: id.into(),
                                severity: yunta_core::events::FindingSeverity::Minor,
                                title: format!("seeded {id}"),
                                location: "src/lib.rs".to_string(),
                                detail: "seeded directly".to_string(),
                                proposed_criterion: None,
                            },
                        },
                    ),
                },
                &yunta_core::SystemClock,
            )
            .unwrap();
    }

    fn findings_by(&self, node: &str) -> Vec<String> {
        self.storage
            .events_for_run(&self.run_id)
            .unwrap()
            .into_iter()
            .filter(|e| e.node_id.as_ref().map(|n| n.as_str()) == Some(node))
            .filter_map(|e| match e.payload() {
                Some(EventPayload::FindingPosted(p)) => Some(p.finding.id.to_string()),
                _ => None,
            })
            .collect()
    }
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
    let bench = Bench::new();
    let session = bench.listener("solo", None).await;

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
    let bench = Bench::new();
    let session = bench.listener("solo", None).await;
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
    assert_eq!(bench.findings_by("solo"), vec!["hot-1"]);

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
    assert_eq!(bench.findings_by("solo").len(), 1);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_tool_written_event_carries_the_runs_injected_clock() {
    // The finding a tool writes is stamped by the run's own clock, not a
    // fresh `SystemClock` — so a run driven by a fixed clock is
    // reproducible through its listener's events too.
    let bench = Bench::with_clock(Arc::new(FrozenClock));
    let session = bench.listener("solo", None).await;
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

    let event = bench
        .storage
        .events_for_run(&bench.run_id)
        .unwrap()
        .into_iter()
        .find(|e| {
            matches!(
                e.payload(),
                Some(EventPayload::FindingPosted(p)) if p.finding.id.as_str() == "clocked"
            )
        })
        .expect("the finding is on the log");
    assert_eq!(event.timestamp, yunta_core::Clock::now(&FrozenClock));
}

// --- yunta_get_blackboard (mount rule, read rule) --------------------

#[tokio::test]
async fn blackboard_is_not_mounted_outside_a_blackboard_group() {
    let bench = Bench::new();
    // `solo` is no group's child; `reviewer` sits in an `independent`
    // group — neither may even see the tool.
    for node in ["solo", "reviewer"] {
        let session = bench.listener(node, None).await;
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
    let bench = Bench::new();
    // A sibling (worker-b) and a foreign node (reviewer) already posted.
    bench.seed_finding("worker-b", "sibling-post");
    bench.seed_finding("reviewer", "foreign-post");

    let session = bench.listener("worker-a", None).await;
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
    client.cancel().await.unwrap();
}

// --- yunta_task_status (read-only tasks view) -------------------------------

#[tokio::test]
async fn task_status_reflects_the_task_state_derived_from_the_log() {
    let bench = Bench::new();
    bench
        .storage
        .append(
            &EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some(NodeId::from("implement")),
                payload: EventPayload::TaskRegistered(yunta_core::events::TaskRegisteredPayload {
                    task_id: TaskId::from("T001"),
                    criteria: Vec::new(),
                    scope: Vec::new(),
                    depends_on: Vec::new(),
                }),
            },
            &yunta_core::SystemClock,
        )
        .unwrap();

    let session = bench.listener("solo", None).await;
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
    let bench = Bench::new();
    let session = bench.listener("implement", Some("T001")).await;
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
    let request = yunta_engine::scope_expansion::load_request(bench._root.path())
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
    let bench = Bench::new();
    let session = bench.listener("solo", None).await;
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

// --- ✓ del plan: concurrent posts, no race -----------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_concurrent_sessions_post_interleaved_without_losing_or_misattributing_any() {
    let bench = Bench::new();
    let session_a = bench.listener("worker-a", None).await;
    let session_b = bench.listener("worker-b", None).await;
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

    let by_a = bench.findings_by("worker-a");
    let by_b = bench.findings_by("worker-b");
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
    let bench = Bench::new();
    let session = bench.listener("solo", None).await;
    let url = session.endpoint.url.clone();
    let token = session.endpoint.token.expose().clone();
    drop(session);
    // Give the graceful shutdown a beat to release the socket.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

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
// same code. These name the two halves of that: what it says when the file
// is right, and that it names the same problems a failed close would.

fn tasks_spec() -> yunta_core::ArtifactSpec {
    yunta_core::ArtifactSpec::Typed {
        name: "plan.yaml".to_string(),
        kind: yunta_core::ArtifactKind::Tasks,
    }
}

#[tokio::test]
async fn a_check_reports_what_the_engine_read_not_only_that_it_parsed() {
    let bench = Bench::new();
    std::fs::write(
        bench.run_dir.join("artifacts").join("plan.yaml"),
        "tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
    )
    .unwrap();
    let session = bench.listener_for("plan", None, vec![tasks_spec()]).await;
    let client = client_for(&session, None).await.unwrap();
    let (is_error, text) = call(&client, "yunta_check_artifact", json!({})).await;

    assert!(!is_error, "got: {text}");
    assert!(text.contains("plan.yaml — ok"), "{text}");
    assert!(
        text.contains("1 task(s) registered: `t1`"),
        "the session sees its meaning survived, not only its syntax: {text}"
    );
}

#[tokio::test]
async fn a_check_names_the_same_problems_the_close_would() {
    let bench = Bench::new();
    // The failure that motivated the tool: a quoted boolean where a
    // boolean belongs. The check reads the file through the same code the
    // close does, so it locates the value and says what was expected.
    std::fs::write(
        bench.run_dir.join("artifacts").join("plan.yaml"),
        "tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    manual_review: \"true\"\n    criteria:\n      - cmd: \"cargo test\"\n",
    )
    .unwrap();
    let session = bench.listener_for("plan", None, vec![tasks_spec()]).await;
    let client = client_for(&session, None).await.unwrap();
    let (_, text) = call(
        &client,
        "yunta_check_artifact",
        json!({"name": "plan.yaml"}),
    )
    .await;

    assert!(text.contains("cannot be read"), "{text}");
    assert!(
        text.contains("tasks[0].manual_review"),
        "the path locates the value: {text}"
    );
    assert!(
        text.contains("expected a boolean"),
        "the same diagnostic the close produces: {text}"
    );
}

#[tokio::test]
async fn a_check_of_a_submitted_document_reads_the_run_not_the_file_beside_it() {
    // Once a document is handed over it is a fact of the run. The verdict
    // is about that document, so a file somebody wrote over afterwards
    // does not change what the session is told.
    let bench = Bench::new();
    let session = bench.listener_for("plan", None, vec![tasks_spec()]).await;
    let client = client_for(&session, None).await.unwrap();
    let (is_error, text) = call(
        &client,
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

    std::fs::write(
        bench.run_dir.join("artifacts").join("plan.yaml"),
        "not a tasks document at all\n",
    )
    .unwrap();
    let (is_error, text) = call(&client, "yunta_check_artifact", json!({})).await;
    assert!(!is_error, "got: {text}");
    assert!(text.contains("plan.yaml — ok"), "{text}");
    assert!(
        text.contains("1 task(s) registered: `t1`"),
        "the verdict is about the document the run holds: {text}"
    );
}

#[tokio::test]
async fn a_check_of_an_artifact_this_node_never_declared_says_which_it_declares() {
    let bench = Bench::new();
    let session = bench.listener_for("plan", None, vec![tasks_spec()]).await;
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
    assert!(text.contains("`plan.yaml`"), "{text}");
}

#[tokio::test]
async fn a_node_with_nothing_to_check_says_so_rather_than_reporting_success() {
    let bench = Bench::new();
    let session = bench.listener("plan", None).await;
    let client = client_for(&session, None).await.unwrap();
    let (is_error, text) = call(&client, "yunta_check_artifact", json!({})).await;
    assert!(is_error, "{text}");
    assert!(text.contains("declares no artifacts"), "{text}");
}
