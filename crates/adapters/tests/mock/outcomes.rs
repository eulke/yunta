//! What a scripted session reports for the steps it plays: a Codex turn's
//! usage and summary, a rejected run tool step that needs no endpoint, a
//! fixture that cannot state a retry for an auth failure, and a session the
//! account's limit stops, with the reset its CLI names when it names one.

use super::*;

/// A Codex turn reports the usage its steps counted on the line that ends the
/// turn, after the summary, wherever the usage steps sit in the script.
#[tokio::test]
async fn a_codex_session_closes_with_its_summary_and_the_usage_it_counted() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::from_yaml(
        r#"
dialect: codex
outcome: { type: completed, summary: "the fixture's summary" }
steps:
  - { type: usage, input_tokens: 10, output_tokens: 5 }
  - { type: note, text: "a note" }
  - { type: usage, input_tokens: 2, output_tokens: 1 }
"#,
    )
    .unwrap();

    let events = drain(mock.spawn(request(dir.path().to_path_buf())).await.unwrap()).await;

    let usages: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Usage {
                input_tokens,
                output_tokens,
                ..
            } => Some((*input_tokens, *output_tokens)),
            _ => None,
        })
        .collect();
    assert_eq!(usages, vec![(Some(12), Some(6))], "{events:?}");
    assert!(
        matches!(
            events.last().unwrap(),
            AgentEvent::Completed { result } if result.summary == "the fixture's summary"
        ),
        "{events:?}"
    );
}

/// A rejected run tool step is reported without a call, so the session needs
/// no endpoint to report it and goes on to its outcome.
#[tokio::test]
async fn a_run_tool_rejected_step_reports_the_rejection_without_a_call() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::from_yaml(
        r#"
dialect: claude-code
outcome: { type: completed, summary: "done" }
steps:
  - { type: run_tool_rejected, tool: yunta_check_artifact }
"#,
    )
    .unwrap();

    let events = drain(mock.spawn(request(dir.path().to_path_buf())).await.unwrap()).await;

    assert!(
        events.contains(&AgentEvent::RunToolRejected {
            tool: RunTool::CheckArtifact
        }),
        "{events:?}"
    );
    assert!(matches!(
        events.last().unwrap(),
        AgentEvent::Completed { .. }
    ));
}

/// A fixture has no key for a retry, so it cannot state one for an auth failure.
#[test]
fn a_fixture_cannot_state_a_retry_for_an_auth_failure() {
    let yaml = "dialect: claude-code\nsessions:\n  - outcome: {type: failed, message: \"401 Unauthorized\", retry: true}";
    assert!(MockFixture::parse_without_a_run(yaml).is_err());
}

/// The instant a scripted limit resets at, in Unix seconds.
const RESETS_AT: i64 = 1_792_022_586;

/// A Claude Code session the account's limit stops ends failed on that
/// limit, with the reset its rate-limit event names.
#[tokio::test]
async fn a_limited_claude_session_ends_on_the_limit_and_the_reset_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::from_yaml(&format!(
        "dialect: claude-code\noutcome: {{ type: limited, resets_at: {RESETS_AT} }}\n"
    ))
    .unwrap();

    let events = drain(mock.spawn(request(dir.path().to_path_buf())).await.unwrap()).await;

    let reset = chrono::DateTime::from_timestamp(RESETS_AT, 0);
    assert!(
        matches!(
            events.last(),
            Some(AgentEvent::Failed { cause: FailureCause::Limit { resets_at }, .. })
                if *resets_at == reset
        ),
        "{events:?}"
    );
}

/// Codex prints no reset for a limit, so a Codex fixture cannot name one,
/// and its limited session ends on a limit with no reset.
#[tokio::test]
async fn a_codex_limit_names_no_reset() {
    let naming = format!(
        "dialect: codex\nsessions:\n  - outcome: {{ type: limited, resets_at: {RESETS_AT} }}\n"
    );
    assert!(MockFixture::parse_without_a_run(&naming).is_err());

    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::from_yaml("dialect: codex\noutcome: { type: limited }\n").unwrap();
    let events = drain(mock.spawn(request(dir.path().to_path_buf())).await.unwrap()).await;

    assert!(
        matches!(
            events.last(),
            Some(AgentEvent::Failed {
                cause: FailureCause::Limit { resets_at: None },
                ..
            })
        ),
        "{events:?}"
    );
}
