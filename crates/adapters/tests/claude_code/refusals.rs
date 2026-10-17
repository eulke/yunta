//! How a Claude Code session's failure is classified: a refused session says
//! whether its refusal is final, an invalid model, or an authentication error,
//! and a limit is read from the stream's own fields, never from its text.

use super::*;
use chrono::DateTime;
use yunta_core::events::{FailureCause, RefusalCause};

#[tokio::test]
async fn a_failed_result_ends_the_stream_with_failed_and_its_cause() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            INIT_LINE,
            r#"{"type":"result","is_error":true,"result":"rate limited","usage":{"input_tokens":5,"output_tokens":0}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.plain.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, cause } => {
            assert_eq!(error.message, "rate limited");
            assert_eq!(cause, &FailureCause::Refused(RefusalCause::Final));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// The message and the refusal a session's failure carries, for a failure the
/// session refused.
fn refused_by(event: &AgentEvent) -> (&str, &RefusalCause) {
    match event {
        AgentEvent::Failed {
            error,
            cause: FailureCause::Refused(cause),
        } => (error.message.as_str(), cause),
        other => panic!("expected a refused Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn an_init_without_a_session_id_is_a_final_refusal() {
    let events = events_of(&[
        r#"{"type":"system","subtype":"init","model":"claude-sonnet-5"}"#,
        r#"{"type":"result","is_error":false,"result":"all done"}"#,
    ])
    .await;

    let (message, cause) = refused_by(&events[0]);
    assert_eq!(cause, &RefusalCause::Final);
    assert!(
        message.contains("session_id"),
        "the failure names the missing field: {message}"
    );
    assert_eq!(
        events.len(),
        1,
        "nothing follows a session's terminal event: {events:?}"
    );
}

#[tokio::test]
async fn an_invalid_model_is_not_retried() {
    let events = events_of(&[
        INIT_LINE,
        r#"{"type":"result","is_error":true,"result":"API Error: 404 {\"type\":\"error\",\"error\":{\"type\":\"not_found_error\",\"message\":\"model: claude-nope\"}}"}"#,
    ])
    .await;

    let (_, cause) = refused_by(events.last().unwrap());
    assert_eq!(cause, &RefusalCause::InvalidModel);
}

#[tokio::test]
async fn an_auth_error_is_refused_for_its_credentials() {
    let events = events_of(&[
        INIT_LINE,
        r#"{"type":"result","is_error":true,"result":"Invalid API key · Please run /login"}"#,
    ])
    .await;

    let (message, cause) = refused_by(events.last().unwrap());
    assert_eq!(cause, &RefusalCause::Authentication, "{message}");
}

#[tokio::test]
async fn a_result_line_without_is_error_is_a_final_refusal() {
    let events = events_of(&[INIT_LINE, r#"{"type":"result","result":"all done"}"#]).await;

    let (message, cause) = refused_by(events.last().unwrap());
    assert_eq!(cause, &RefusalCause::Final);
    assert!(
        message.contains("is_error"),
        "the failure names the missing field: {message}"
    );
}

#[tokio::test]
async fn an_event_before_the_session_opens_fails_the_session() {
    let events = events_of(&[
        r#"{"type":"assistant","message":{"id":"msg-0","content":[{"type":"text","text":"hi"}]}}"#,
        INIT_LINE,
        r#"{"type":"result","is_error":false,"result":"all done"}"#,
    ])
    .await;

    let (message, cause) = refused_by(&events[0]);
    assert_eq!(cause, &RefusalCause::Final);
    assert!(
        message.contains("before"),
        "the failure says what came before the session opened: {message}"
    );
    assert_eq!(events.len(), 1, "got: {events:?}");
}

/// The cause a stream ends on.
fn last_cause(events: &[AgentEvent]) -> &FailureCause {
    match events.last() {
        Some(AgentEvent::Failed { cause, .. }) => cause,
        other => panic!("expected the stream to end on a failure, got {other:?}"),
    }
}

/// The limit is read from the stream's own fields, never from the text: the
/// captures' text says "temporarily limiting requests" in both forms.
#[tokio::test]
async fn a_claude_limit_is_read_from_its_stream_fields() {
    let rejected = DateTime::from_timestamp(1791730875, 0);
    for capture in [
        include_str!("../fixtures/limits/claude-429-reset.txt"),
        include_str!("../fixtures/limits/claude-429-reset-body.txt"),
    ] {
        let lines: Vec<&str> = capture.lines().collect();
        let events = events_of(&lines).await;
        assert_eq!(
            last_cause(&events),
            &FailureCause::Limit {
                resets_at: rejected
            }
        );
    }
    for capture in [
        include_str!("../fixtures/limits/claude-429-plain.txt"),
        include_str!("../fixtures/limits/claude-429-body.txt"),
    ] {
        let lines: Vec<&str> = capture.lines().collect();
        let events = events_of(&lines).await;
        assert_eq!(
            last_cause(&events),
            &FailureCause::Limit { resets_at: None }
        );
    }
    // An allowed window's reset is not the limit's, whichever comes last.
    let events = events_of(&[
        INIT_LINE,
        r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1791730875}}"#,
        r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1791744000}}"#,
        r#"{"type":"assistant","error":"rate_limit","message":{"content":[]}}"#,
        r#"{"type":"result","is_error":true,"api_error_status":429,"result":"API Error: Server is temporarily limiting requests (not your usage limit)"}"#,
    ])
    .await;
    assert_eq!(
        last_cause(&events),
        &FailureCause::Limit {
            resets_at: rejected
        }
    );
    // The text alone is not a limit.
    let events = events_of(&[
        INIT_LINE,
        r#"{"type":"result","is_error":true,"result":"API Error: Server is temporarily limiting requests (not your usage limit)"}"#,
    ])
    .await;
    assert_eq!(
        last_cause(&events),
        &FailureCause::Refused(RefusalCause::Final)
    );
}

/// A field the CLI filled with something that cannot be a session id
/// fails the session, and the failure keeps what rejected the value so a
/// reader following the chain reaches the rule it broke.
#[tokio::test]
async fn a_session_id_that_cannot_be_one_fails_keeping_what_rejected_it() {
    let events = events_of(&[
        r#"{"type":"system","subtype":"init","session_id":"","model":"claude-sonnet-5"}"#,
    ])
    .await;

    let AgentEvent::Failed { error, cause } = &events[0] else {
        panic!("expected Failed, got {:?}", events[0]);
    };
    assert_eq!(
        cause,
        &FailureCause::Refused(RefusalCause::Final),
        "{error}"
    );
    let described = yunta_core::describe(error);
    assert!(
        described.starts_with("the CLI's init line's `session_id`"),
        "the failure names the field: {described}"
    );
    assert!(
        error.source().is_some(),
        "the failure keeps what rejected the value: {described}"
    );
    assert!(
        described.len() > error.message.len(),
        "the cause is read, not dropped: {described}"
    );
}
