use yunta_core::events::SessionEvent;
use yunta_testkit::Bench;

/// A run tool the CLI reports as rejected, when no call of that tool
/// reached the server, was blocked before it got there.
#[tokio::test]
async fn a_run_tool_refused_before_the_server_is_approval_blocked() {
    let workflow = "name: rejected\nnodes:\n  - id: ask\n    kind: prompt\n    runner: executor\n    prompt: \"Post the finding.\"\n";
    let fixture = "dialect: claude-code\nsessions:\n  - steps:\n      - type: run_tool_rejected\n        tool: yunta_post_finding\n    outcome: { type: completed, summary: done }\n";
    let bench = Bench::new();
    bench.run(workflow, fixture).await;
    let causes: Vec<yunta_core::events::RunToolFailureCause> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(yunta_core::events::EventPayload::Session(SessionEvent::RunToolFailed(p))) => {
                Some(p.cause)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        causes,
        vec![yunta_core::events::RunToolFailureCause::ApprovalBlocked],
        "a refusal the server never saw"
    );
}
