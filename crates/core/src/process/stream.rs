//! The order a CLI's events keep, as they come out of its stream.
//!
//! A session's stream opens with its `SessionOpened` and ends with exactly
//! one terminal event. A line that reports work before the session opened
//! is not the protocol, and nothing after the terminal event belongs to the
//! session. [`StreamGuard`] holds both rules for every adapter, so a parser
//! only has to translate each line.

use crate::port::{AgentError, AgentEvent, FailureCause};

/// Holds one stream to the protocol while its events are read: the first
/// event that is not a session opening fails the stream, and a terminal
/// event closes it.
#[derive(Debug, Default)]
pub struct StreamGuard {
    opened: bool,
    closed: bool,
}

impl StreamGuard {
    /// Admits one parsed event, or returns what replaces it; `None` once the
    /// stream has ended.
    pub fn admit(&mut self, event: AgentEvent, cli: &str) -> Option<AgentEvent> {
        if self.closed {
            return None;
        }
        let event = match event {
            AgentEvent::SessionOpened { .. } | AgentEvent::Failed { .. } => event,
            other if self.opened => other,
            // The session opens before anything happens in it: a CLI that
            // reports work under no session is not speaking the protocol,
            // and a retry would read the same lines.
            other => AgentEvent::Failed {
                error: AgentError::message(format!(
                    "the {cli} CLI reported {} before opening the session",
                    describe(&other)
                )),
                cause: FailureCause::Final,
            },
        };
        self.opened = true;
        self.closed = matches!(
            event,
            AgentEvent::Completed { .. } | AgentEvent::Failed { .. }
        );
        Some(event)
    }
}

/// An event's kind, for a message about one that came out of order.
fn describe(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::SessionOpened { .. } => "a session opening",
        AgentEvent::ToolUse { .. } => "a tool use",
        AgentEvent::Usage { .. } => "token usage",
        AgentEvent::Note { .. } => "a note",
        AgentEvent::Completed { .. } => "a completion",
        AgentEvent::Failed { .. } => "a failure",
        AgentEvent::RunToolFailed { .. } => "a run tool failure",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port::AgentOutcome;
    use crate::SessionId;

    fn opened() -> AgentEvent {
        AgentEvent::SessionOpened {
            session_id: SessionId::from_static("session-1"),
            model: None,
            fence: None,
        }
    }

    /// A line that reports work before the session opened fails the stream,
    /// naming the CLI and the kind of event it reported.
    #[test]
    fn work_before_the_session_opens_fails_the_stream() {
        let mut guard = StreamGuard::default();
        let note = AgentEvent::Note {
            text: "early".to_string(),
        };
        let Some(AgentEvent::Failed { error, cause }) = guard.admit(note, "probe") else {
            panic!("a note before the session opens fails the stream");
        };
        assert_eq!(
            error.message,
            "the probe CLI reported a note before opening the session"
        );
        assert_eq!(cause, FailureCause::Final);
    }

    /// Once a terminal event has been admitted, whatever comes after it is
    /// not part of the session.
    #[test]
    fn nothing_is_admitted_after_the_terminal_event() {
        let mut guard = StreamGuard::default();
        assert!(guard.admit(opened(), "probe").is_some());
        let done = AgentEvent::Completed {
            result: AgentOutcome {
                summary: "done".to_string(),
            },
        };
        assert_eq!(guard.admit(done.clone(), "probe"), Some(done));
        let late = AgentEvent::Note {
            text: "late".to_string(),
        };
        assert_eq!(guard.admit(late, "probe"), None);
    }

    /// Once the session is open, its events pass as they are.
    #[test]
    fn events_after_the_opening_pass_unchanged() {
        let mut guard = StreamGuard::default();
        assert!(guard.admit(opened(), "probe").is_some());
        let note = AgentEvent::Note {
            text: "working".to_string(),
        };
        assert_eq!(guard.admit(note.clone(), "probe"), Some(note));
    }
}
