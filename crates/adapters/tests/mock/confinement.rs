//! What a mock session reports of its confinement: a full session's commands run
//! unconfined, so the coverage it reports names no root.

use super::*;
use yunta_core::fence::Coverage;

#[tokio::test]
async fn a_mock_full_session_reports_its_commands_unconfined() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::from_yaml(
        "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: ok }",
    )
    .unwrap();
    let mut req = request(dir.path().to_path_buf());
    confine(
        &mut req,
        PermissionProfile::Full,
        EditSandbox::default(),
        false,
        true,
    )
    .unwrap();
    let events = drain(mock.spawn(req).await.unwrap()).await;
    let Some(AgentEvent::SessionOpened { fence, .. }) = events.first() else {
        panic!("{events:?}")
    };
    assert_eq!(fence.as_ref(), Some(&Coverage::ToolsOnly));
}
