//! A write the engine cannot be asked about, and the session that asked for it.

use super::*;

/// A write the engine cannot be asked about is refused, and the session does
/// not fail for it: the refusal is the write's answer, not a session error.
#[tokio::test]
async fn a_write_the_engine_cannot_be_asked_about_is_refused_not_a_failed_session() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = MockAdapter::from_yaml(
        "dialect: claude-code\neffects:\n  - { path: src/lib.rs, content: \"x\" }\noutcome: { type: completed, summary: done }\n",
    )
    .unwrap();
    let mut req = request(dir.path().to_path_buf());
    // Nothing listens on the port this listener held and then released.
    let nobody = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    req.rail = Some(RailEndpoint {
        url: format!("http://{nobody}/rail"),
        token: Secret::new("t".to_string()),
    });
    let events = drain(fixture.spawn(req).await.unwrap()).await;
    assert!(
        !matches!(events.last(), Some(AgentEvent::Failed { .. })),
        "{events:?}"
    );
    assert!(!dir.path().join("src/lib.rs").exists());
}
