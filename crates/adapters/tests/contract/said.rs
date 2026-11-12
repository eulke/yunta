//! What a session says reaches its stream whole and in order under every
//! CLI, and what it reasons never does: a reader downstream gets a session's
//! words the same way whichever adapter it ran on.

use yunta_core::port::AgentEvent;

use super::cases::{completed, fresh};
use super::subject::{Step, Subject};

/// A session says a text of several lines with Markdown, reasons, then says
/// a second text. Its stream carries both texts whole, in that order, and
/// nothing of the reasoning.
pub async fn what_a_session_says_arrives_whole_and_in_order_and_never_its_reasoning(
    subject: &dyn Subject,
) {
    let first = "## The parser\n\nI read `parse.rs`:\n- the lexer\n- the *empty* input";
    let second = "Adding a case for it now.";
    let steps = vec![
        Step::Note(first.to_string()),
        Step::Reasoning("kept to itself".to_string()),
        Step::Note(second.to_string()),
    ];
    let (dir, req) = fresh();

    let played = subject.play(dir.path(), req, &completed(steps)).await;

    let said: Vec<&str> = played
        .events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Note { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(said[..2], [first, second], "{:?}", played.events);
    assert!(
        !format!("{:?}", played.events).contains("kept to itself"),
        "the reasoning reached the stream: {:?}",
        played.events
    );
}
