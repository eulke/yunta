//! A CLI that retries a request to its service prints a notice and goes on,
//! and the notice ends nothing under any adapter: the session ends as its
//! turn does.

use yunta_core::port::AgentEvent;

use super::cases::{completed, fresh};
use super::subject::{Step, Subject};

/// A session says something, prints a retry notice, says something more and
/// completes. Its stream ends `Completed` with the script's summary, after
/// both things said, and nothing on it failed.
pub async fn a_retry_notice_ends_nothing(subject: &dyn Subject) {
    let steps = vec![
        Step::Note("before the retry".to_string()),
        Step::RetryNotice,
        Step::Note("after the retry".to_string()),
    ];
    let (dir, req) = fresh();

    let played = subject.play(dir.path(), req, &completed(steps)).await;

    assert!(
        !played
            .events
            .iter()
            .any(|event| matches!(event, AgentEvent::Failed { .. })),
        "{:?}",
        played.events
    );
    let said: Vec<&str> = played
        .events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Note { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        said[..2],
        ["before the retry", "after the retry"],
        "{:?}",
        played.events
    );
    assert!(
        matches!(played.events.last(), Some(AgentEvent::Completed { result }) if result.summary == "ok"),
        "{:?}",
        played.events
    );
}
