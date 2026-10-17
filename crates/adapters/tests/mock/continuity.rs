//! What a mock keeps from one invocation to the next: a session whose script
//! ended answers `kill` as stopped, and a ledger of claims lets a mock built
//! later from the same fixture serve the scripts the first one left unclaimed.

use super::*;
use yunta_adapters::MOCK_CLAIMS_FILE;
use yunta_core::AdapterError;

/// Two scripts, each completing with its own summary.
const TWO_SCRIPTS: &str = r#"
dialect: claude-code
sessions:
  - outcome: { type: completed, summary: first }
  - outcome: { type: completed, summary: second }
"#;

/// One script, which does not hold the two a ledger of `TWO_SCRIPTS` claims.
const ONE_SCRIPT: &str = r#"
dialect: claude-code
outcome: { type: completed, summary: only }
"#;

/// The ordered stop ends this script, so nothing is left running for the
/// refused kill to refuse.
#[tokio::test]
async fn a_refused_kill_answers_ok_once_an_interrupt_ended_the_script() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::from_yaml(
        r#"
dialect: claude-code
sessions:
  - kill: refuse
    outcome: { type: hang, on_interrupt: end }
"#,
    )
    .unwrap();
    let mut session = mock.spawn(request(dir.path().to_path_buf())).await.unwrap();

    session.interrupt().await.unwrap();
    let events: Vec<AgentEvent> = session.events().collect().await;

    assert!(
        matches!(events.as_slice(), [AgentEvent::SessionOpened { .. }]),
        "{events:?}"
    );
    let killed = session.kill().await;
    assert!(
        killed.is_ok(),
        "a kill after the script ended is refused: {killed:?}"
    );
}

/// A stop the script ignores leaves the session playing, so the refused kill
/// says so and the stream stays open.
#[tokio::test]
async fn a_refused_kill_leaves_a_session_that_ignores_interrupt_running() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::from_yaml(
        r#"
dialect: claude-code
sessions:
  - kill: refuse
    outcome: { type: hang, on_interrupt: ignore }
"#,
    )
    .unwrap();
    let mut session = mock.spawn(request(dir.path().to_path_buf())).await.unwrap();

    session.interrupt().await.unwrap();
    assert!(session.kill().await.is_err(), "a refused kill is an error");

    let mut events = session.events();
    let opened = events.next().await;
    assert!(
        matches!(opened, Some(AgentEvent::SessionOpened { .. })),
        "{opened:?}"
    );
    assert!(
        futures::poll!(events.next()).is_pending(),
        "the stream stays open"
    );
}

/// One session of `mock`, opened in `dir` and drained to its end.
async fn play_once(mock: &MockAdapter, dir: &Path) -> Vec<AgentEvent> {
    let session = mock.spawn(request(dir.to_path_buf())).await.unwrap();
    drain(session).await
}

/// The session id a stream opens with.
fn opened_as(events: &[AgentEvent]) -> SessionId {
    match events.first() {
        Some(AgentEvent::SessionOpened { session_id, .. }) => session_id.clone(),
        other => panic!("a session opens with its id: {other:?}"),
    }
}

/// A mock kept at a ledger another mock wrote serves the script that mock left
/// unclaimed, under a session id that mock never gave.
#[tokio::test]
async fn a_mock_kept_at_a_ledger_serves_the_script_the_first_left_unclaimed() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join(MOCK_CLAIMS_FILE);
    let first = MockAdapter::from_yaml(TWO_SCRIPTS)
        .unwrap()
        .keeping_claims(ledger.clone())
        .unwrap();
    let first_events = play_once(&first, dir.path()).await;

    let second = MockAdapter::from_yaml(TWO_SCRIPTS)
        .unwrap()
        .keeping_claims(ledger)
        .unwrap();
    let second_events = play_once(&second, dir.path()).await;

    assert!(
        matches!(
            second_events.last(),
            Some(AgentEvent::Completed { result }) if result.summary == "second"
        ),
        "{second_events:?}"
    );
    assert_ne!(opened_as(&second_events), opened_as(&first_events));
}

/// A ledger that claims a script the fixture does not hold is refused: it
/// records the claims of some other fixture.
#[tokio::test]
async fn a_ledger_naming_a_script_the_fixture_does_not_hold_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join(MOCK_CLAIMS_FILE);
    let two = MockAdapter::from_yaml(TWO_SCRIPTS)
        .unwrap()
        .keeping_claims(ledger.clone())
        .unwrap();
    for _ in 0..2 {
        play_once(&two, dir.path()).await;
    }

    let refused = MockAdapter::from_yaml(ONE_SCRIPT)
        .unwrap()
        .keeping_claims(ledger)
        .err();

    assert!(
        refused
            .as_ref()
            .is_some_and(|error| error.to_string().contains("script 1")),
        "{refused:?}"
    );
}

/// A file that is not a ledger is refused, rather than read as an empty one.
#[test]
fn a_file_that_is_not_a_ledger_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join(MOCK_CLAIMS_FILE);
    std::fs::write(&ledger, "{ not a ledger").unwrap();

    let refused = MockAdapter::from_yaml(TWO_SCRIPTS)
        .unwrap()
        .keeping_claims(ledger)
        .err();

    assert!(
        refused
            .as_ref()
            .is_some_and(|error| error.to_string().contains("not a claims ledger")),
        "{refused:?}"
    );
}

/// A claim whose ledger cannot be written fails the spawn, and the script it
/// would have claimed stays unclaimed.
#[tokio::test]
async fn a_claim_whose_ledger_cannot_be_written_fails_the_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("no-such-directory").join(MOCK_CLAIMS_FILE);
    let mock = MockAdapter::from_yaml(TWO_SCRIPTS)
        .unwrap()
        .keeping_claims(ledger)
        .unwrap();

    let refused = mock.spawn(request(dir.path().to_path_buf())).await.err();

    assert!(
        matches!(refused, Some(AdapterError::AdapterIo { .. })),
        "{refused:?}"
    );
    assert_eq!(mock.unconsumed(), vec![0, 1]);
}
