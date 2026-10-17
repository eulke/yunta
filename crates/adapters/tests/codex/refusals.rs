//! How a Codex session's failure is classified: the cause a failed turn ends
//! on, a limit read from the stream, an authentication error refused for its
//! credentials, a fatal stream error, and a thread id the CLI states that
//! cannot be one.

use super::*;

#[tokio::test]
async fn a_failed_turn_ends_the_stream_with_failed_and_its_cause() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            r#"{"type":"turn.failed","error":{"message":"model response stream ended unexpectedly"}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, cause } => {
            assert_eq!(error.message, "model response stream ended unexpectedly");
            assert_eq!(cause, &FailureCause::Refused(RefusalCause::Unreachable));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// The limit a Codex stream reports is read from the stream, and it is not
/// re-attempted: the first failure of each capture is the account's limit.
#[tokio::test]
async fn a_codex_limit_is_read_from_its_json_stream() {
    for capture in [
        include_str!("../fixtures/limits/codex-json-429-plain.jsonl"),
        include_str!("../fixtures/limits/codex-json-429-body.jsonl"),
    ] {
        let lines: Vec<&str> = capture.lines().collect();
        let events = events_of(&lines).await;
        // The model warning is an item, not a failure: the first failure is the limit's.
        let first = events.iter().find_map(|event| match event {
            AgentEvent::Failed { cause, .. } => Some(cause),
            _ => None,
        });
        assert_eq!(
            first,
            Some(&FailureCause::Limit { resets_at: None }),
            "{events:?}"
        );
    }
}

#[tokio::test]
async fn an_auth_error_is_refused_for_its_credentials() {
    let events = events_of(&[
        THREAD_STARTED_LINE,
        r#"{"type":"turn.failed","error":{"message":"unexpected status 401 Unauthorized: Missing bearer or basic authentication in header"}}"#,
    ])
    .await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, cause } => {
            assert_eq!(
                cause,
                &FailureCause::Refused(RefusalCause::Authentication),
                "{}",
                error.message
            );
            assert!(error.message.contains("401"));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn a_fatal_error_event_ends_the_session_as_failed() {
    let events = events_of(&[
        THREAD_STARTED_LINE,
        r#"{"type":"error","message":"stream disconnected before completion"}"#,
    ])
    .await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, cause } => {
            assert_eq!(
                cause,
                &FailureCause::Refused(RefusalCause::Unreachable),
                "a transport failure is the one the engine re-attempts"
            );
            assert_eq!(error.message, "stream disconnected before completion");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// A thread id the CLI states and nothing can be fails the session, and
/// the failure keeps what rejected the value so a reader following the
/// chain reaches the rule it broke.
#[tokio::test]
async fn a_thread_id_that_cannot_be_one_fails_keeping_what_rejected_it() {
    let events = events_of(&[r#"{"type":"thread.started","thread_id":""}"#]).await;

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
        described.starts_with("the CLI's `thread.started` line is malformed"),
        "the failure names the line: {described}"
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
