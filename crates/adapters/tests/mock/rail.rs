//! The `/rail` judge a fixture's effects are checked by, and the fence tests
//! that a session's writes are refused or allowed through it.

use super::*;

/// A `/rail` that rules as the engine's rail does, with one route per moment:
/// a write is judged by the judge's fence under its confinement, and each path
/// it refuses is answered with the fence's own refusal, named as the adapter
/// names tools.
async fn judging_rail(judge: Judge) -> RailEndpoint {
    let app = Router::new()
        .route("/rail/before-write", post(judge_write))
        .route("/rail/after-write", post(judge_after_write))
        .route("/rail/before-stop", post(judge_before_stop))
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

/// The engine's answer to a write: it is refused when the fence refuses any
/// path it names, judged under the confinement, and the reason is the engine's
/// own wording of those refusals.
async fn judge_write(State(judge): State<Judge>, Json(call): Json<RailCall>) -> Json<WriteAnswer> {
    let RailAct::BeforeWrite { paths } = call.act else {
        return Json(WriteAnswer::Proceed);
    };
    let refusals: Vec<_> = paths
        .iter()
        .filter_map(
            |path| match judge.fence.judge(&judge.worktree, path, &judge.confinement) {
                Verdict::Refused(refusal) => Some(refusal),
                Verdict::Allowed => None,
            },
        )
        .collect();
    Json(match WriteRefusal::of(&refusals, judge.naming) {
        Some(reason) => WriteAnswer::Refuse { reason },
        None => WriteAnswer::Proceed,
    })
}

/// The judge formats nothing, so no write is told anything after it lands.
async fn judge_after_write(Json(_): Json<RailCall>) -> Json<FormatAnswer> {
    Json(FormatAnswer::Proceed)
}

/// The judge lets every stop go on.
async fn judge_before_stop(Json(_): Json<RailCall>) -> Json<StopAnswer> {
    Json(StopAnswer::Proceed)
}

/// The request with its hooks pointed at a judge of its own fence, which names
/// tools as `adapter` does. A session scripted to write asks before each
/// write, as a CLI's write hook does, and a write the fence refuses is not made.
pub(super) async fn judged(mut req: SessionRequest, adapter: &MockAdapter) -> SessionRequest {
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

/// The same refusal under the codex dialect, whose hook is asked about each
/// file the patch names.
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
