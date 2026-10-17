//! A judgement its session's stop cuts short: what the judgement runs stops
//! with the session, nothing failed, and the log records no failure for it.

use super::rail_calls::post_rail;
use super::*;

/// A task whose one criterion runs until its session's stop ends it.
fn slow_task() -> yunta_core::Task {
    yunta_core::yaml::parse(
        r#"
id: T001
title: Write the greeting
scope: [hello.txt]
criteria:
  - cmd: sleep 30
"#,
    )
    .unwrap()
}

/// Whether the log holds the question a check of the task asked.
fn check_started(host: &ToolsHost) -> bool {
    host.events().iter().any(|event| {
        matches!(
            event.payload(),
            Some(EventPayload::Tasks(TaskEvent::CheckStarted(_)))
        )
    })
}

/// A session that stops while its stop is being judged (its timeout ended
/// it, or its task stopped) cuts the judgement short. The judgement did not
/// fail, so the session is let stop and the log holds no rail failure.
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_judgement_its_sessions_stop_cuts_short_is_no_rail_failure() {
    // `post_rail` blocks its thread, so the listener needs another one to answer on.
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
    let access = host.task_access(slow_task(), unit);
    let stop = access.cancel.clone();
    let session = host.task_session("implement", access).await;
    session.rails().own("listener-session".parse().unwrap());
    let endpoint = session.rail_endpoint();
    let body = json!({
        "session_id": "listener-session",
        "act": { "event": "before-stop" },
    });
    let asking = tokio::task::spawn_blocking(move || {
        post_rail(&endpoint, "/rail/before-stop", &body.to_string())
    });
    yunta_testkit::wait_until_async(
        || std::future::ready(check_started(&host)),
        || "the stop judgement never started".to_string(),
    )
    .await;

    stop.cancel();
    let answer = asking.await.unwrap();

    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    let failed: Vec<String> = host
        .events()
        .iter()
        .filter_map(|event| event.payload())
        .filter(|payload| payload.kind_name() == "rail_failed")
        .map(|payload| format!("{payload:?}"))
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
}
