//! A finding the engine reported about the run itself reads with no node beside it.

use super::*;

#[tokio::test]
async fn an_engine_finding_reads_with_no_node() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    host.record(
        None,
        EventPayload::Findings(FindingEvent::Posted(
            yunta_core::events::FindingPostedPayload {
                finding: yunta_core::events::Finding {
                    id: "cleanup".into(),
                    severity: yunta_core::events::FindingSeverity::Minor,
                    says: yunta_core::events::FindingSays::Written {
                        title: "a cleanup did not finish".to_string(),
                        detail: "left behind".to_string(),
                    },
                    location: "run:scratch".into(),
                    proposed_criterion: None,
                },
            },
        )),
    );
    let session = host.session("fix", None).await;
    let client = client_for(&session, None).await.unwrap();

    let (_, text) = call(&client, "yunta_findings", json!({})).await;
    client.cancel().await.unwrap();

    let standing: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(standing["findings"][0].get("node").is_none(), "{text}");
    assert_eq!(standing["findings"][0]["id"], "cleanup");
}
