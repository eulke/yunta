//! A mock session that prints the notice its dialect's CLI prints when it
//! retries a request goes on, as that CLI goes on, and ends as its outcome
//! says.

use yunta_adapters::MockAdapter;
use yunta_core::port::{Adapter, AgentEvent};
use yunta_testkit_core::adapter::{drain, request};

/// Under each dialect, a session that retries and then completes ends
/// `Completed` with its summary, and nothing on its stream failed.
#[tokio::test]
async fn a_mock_session_that_retries_completes_in_either_dialect() {
    for dialect in ["claude-code", "codex"] {
        let dir = tempfile::tempdir().unwrap();
        let mock = MockAdapter::from_yaml(&format!(
            "dialect: {dialect}\nsteps:\n  - {{ type: note, text: \"before\" }}\n  - {{ type: retry_notice }}\n  - {{ type: note, text: \"after\" }}\noutcome: {{ type: completed, summary: done }}\n"
        ))
        .unwrap_or_else(|error| panic!("{dialect}: {error}"));

        let events = drain(mock.spawn(request(dir.path().to_path_buf())).await.unwrap()).await;

        assert!(
            !events
                .iter()
                .any(|event| matches!(event, AgentEvent::Failed { .. })),
            "{dialect}: {events:?}"
        );
        assert!(
            matches!(events.last(), Some(AgentEvent::Completed { result }) if result.summary == "done"),
            "{dialect}: {events:?}"
        );
    }
}
