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
        // The directories `create_run` gives every run: the view the
        // engine writes, and the working space every node stages in.
        for dir in [
            yunta_core::ARTIFACTS_DIR,
            yunta_engine::run_dir::SCRATCH_DIR,
        ] {
            std::fs::create_dir_all(run_dir.join(dir)).unwrap();
        }
        let host = Arc::new(RunToolsHost::new(
            storage.async_handle(),
            run_id.clone(),
            &workflow,
            clock,
            // These tests read what a tool call lands on the log; the
            // mirror of it has its own test (`observer.rs`).
            None,
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

    /// Where `node` writes what it declares, created as an attempt of
    /// that node would create it.
    fn staging(&self, node: &str) -> std::path::PathBuf {
        let dir = yunta_engine::run_dir::staging(&self.run_dir, &node.into());
        std::fs::create_dir_all(&dir).unwrap();
        dir
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
    let bench = Bench::new();
    let session = bench.listener_for("plan", None, vec![tasks_spec()]).await;
    let client = client_for(&session, None).await.unwrap();
    submit_plan(&client).await;

    let (is_error, text) = call(&client, "yunta_check_artifact", json!({})).await;
    assert!(!is_error, "got: {text}");
    assert!(text.contains("tasks — ok"), "{text}");
    assert!(
        text.contains("1 task(s) registered: `t1`"),
        "the session sees its meaning survived, not only its syntax: {text}"
    );
}

#[tokio::test]
async fn a_check_before_the_document_is_handed_over_says_what_the_close_would() {
    let bench = Bench::new();
    // A tasks document written by hand where a command node writes one.
    // This node's document arrives through the tool, so the file is not
    // it — and the session is told exactly what its close would say,
    // word for word, instead of a confidence the close will not honour.
    std::fs::write(
        bench.staging("plan").join("tasks.yaml"),
        "tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
    )
    .unwrap();
    let session = bench.listener_for("plan", None, vec![tasks_spec()]).await;
    let client = client_for(&session, None).await.unwrap();
    let (_, text) = call(&client, "yunta_check_artifact", json!({"name": "tasks"})).await;

    let close = yunta_engine::close_artifacts(&plan_node(), &bench.run_dir, &[], None)
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
    let bench = Bench::new();
    let session = bench.listener_for("plan", None, vec![tasks_spec()]).await;
    let client = client_for(&session, None).await.unwrap();
    submit_plan(&client).await;

    std::fs::write(
        bench.staging("plan").join("tasks.yaml"),
        "not a tasks document at all\n",
    )
    .unwrap();
    let (is_error, text) = call(&client, "yunta_check_artifact", json!({})).await;
    assert!(!is_error, "got: {text}");
    assert!(text.contains("tasks — ok"), "{text}");
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
    assert!(text.contains("`tasks`"), "{text}");
}

/// A node declares the kind, so the tool that submits it is either
/// mounted or absent: there is no argument left for a session to get
/// wrong, and a document this node's close will never look for has no
/// tool to arrive through.
#[tokio::test]
async fn only_the_kinds_this_node_declares_have_a_submission_tool() {
    let bench = Bench::new();
    let session = bench.listener_for("plan", None, vec![tasks_spec()]).await;
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
    let bench = Bench::new();
    let session = bench.listener("plan", None).await;
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
    let bench = Bench::new();
    let session = bench.listener("solo", None).await;

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
    let bench = Bench::new();
    let session = bench.listener("solo", None).await;

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
