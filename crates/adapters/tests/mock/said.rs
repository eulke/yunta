//! What a mock session says, and what it keeps to itself: its notes reach
//! the stream as its dialect's CLI prints them, and its reasoning, which
//! that CLI prints too, never does.

use yunta_adapters::MockAdapter;
use yunta_core::port::{Adapter, AgentEvent};
use yunta_testkit_core::adapter::{drain, request};

/// Under each dialect, a session that says something, reasons, then says
/// something else streams the two things it said, in order, and nothing of
/// what it reasoned.
#[tokio::test]
async fn a_mock_sessions_reasoning_never_reaches_its_stream() {
    for dialect in ["claude-code", "codex"] {
        let dir = tempfile::tempdir().unwrap();
        let mock = MockAdapter::from_yaml(&format!(
            "dialect: {dialect}\nsteps:\n  - {{ type: note, text: \"first said\" }}\n  - {{ type: reasoning, text: \"kept to itself\" }}\n  - {{ type: note, text: \"then said\" }}\noutcome: {{ type: completed, summary: done }}\n"
        ))
        .unwrap_or_else(|error| panic!("{dialect}: {error}"));

        let events = drain(mock.spawn(request(dir.path().to_path_buf())).await.unwrap()).await;

        let said: Vec<&str> = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::Note { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            said[..2],
            ["first said", "then said"],
            "{dialect}: {events:?}"
        );
        assert!(
            !format!("{events:?}").contains("kept to itself"),
            "{dialect}: the reasoning reached the stream: {events:?}"
        );
    }
}
