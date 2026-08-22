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
use yunta_core::events::{Event, EventPayload};
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
    storage: Storage,
    run_id: RunId,
    host: Arc<RunToolsHost>,
}

impl Bench {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
        let run_id = RunId::from("run-tools-1");
        let workflow: Workflow = serde_yaml::from_str(BLACKBOARD_WORKFLOW).unwrap();
        // Every log opens with run_created — the listener's own
        // appends land on an already-born run in production too.
        storage
            .append_event(&Event {
                run_id: run_id.clone(),
                seq: 0,
                timestamp: chrono::Utc::now(),
                node_id: None,
                payload: EventPayload::RunCreated(yunta_core::events::RunCreatedPayload {
                    manifest_hash: "test-manifest".to_string(),
                    inputs: Default::default(),
                    mode: "default".to_string(),
                    promoted_from: None,
                    yunta_schema: None,
                    base_branch: "main".to_string(),
                    base_commit: "0000".to_string(),
                }),
            })
            .unwrap();
        let host = Arc::new(RunToolsHost::new(
            storage.reopen().unwrap(),
            run_id.clone(),
            &workflow,
        ));
        Bench {
            _root: root,
            storage,
            run_id,
            host,
        }
    }

    async fn listener(&self, node: &str, task: Option<&str>) -> RunToolsSession {
        open_session_listener(
            self.host.clone(),
            NodeId::from(node),
            task.map(TaskId::from),
            self._root.path().to_path_buf(),
        )
        .await
        .unwrap()
    }

    fn seed_finding(&self, node: &str, id: &str) {
        self.storage
            .append_event(&Event {
                run_id: self.run_id.clone(),
                seq: 0,
                timestamp: chrono::Utc::now(),
                node_id: Some(NodeId::from(node)),
                payload: EventPayload::FindingPosted(yunta_core::events::FindingPostedPayload {
                    finding: yunta_core::events::Finding {
                        id: id.to_string(),
                        severity: yunta_core::events::FindingSeverity::Minor,
                        title: format!("seeded {id}"),
                        location: "src/lib.rs".to_string(),
                        detail: "seeded directly".to_string(),
                        proposed_criterion: None,
                    },
                }),
            })
            .unwrap();
    }

    fn findings_by(&self, node: &str) -> Vec<String> {
        self.storage
            .events_for_run(&self.run_id)
            .unwrap()
            .into_iter()
            .filter(|e| e.node_id.as_ref().map(|n| n.as_str()) == Some(node))
            .filter_map(|e| match e.payload {
                EventPayload::FindingPosted(p) => Some(p.finding.id),
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
    let token = token_override.unwrap_or(&session.endpoint.token);
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
        assert!(text.contains("blackboard"), "got: {text}");
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
    assert!(text.contains("own-post"), "got: {text}");
    assert!(!text.contains("sibling-post"), "got: {text}");
    assert!(!text.contains("foreign-post"), "got: {text}");
    client.cancel().await.unwrap();
}

// --- yunta_task_status (read-only ledger view) -------------------------------

#[tokio::test]
async fn task_status_reflects_the_ledger_derived_from_the_log() {
    let bench = Bench::new();
    bench
        .storage
        .append_event(&Event {
            run_id: bench.run_id.clone(),
            seq: 0,
            timestamp: chrono::Utc::now(),
            node_id: Some(NodeId::from("implement")),
            payload: EventPayload::TaskRegistered(yunta_core::events::TaskRegisteredPayload {
                task_id: TaskId::from("T001"),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            }),
        })
        .unwrap();

    let session = bench.listener("solo", None).await;
    let client = client_for(&session, None).await.unwrap();
    let (is_error, text) = call(&client, "yunta_task_status", json!({})).await;
    assert!(!is_error, "got: {text}");
    assert!(text.contains("T001"), "got: {text}");
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
    assert!(text.contains("already pending"), "got: {text}");

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
    assert!(text.contains("task"), "got: {text}");
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
    let token = session.endpoint.token.clone();
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
