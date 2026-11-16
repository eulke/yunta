//! A mock session held by a word: it reads what comes before the word, stands there
//! until the test gives it, and ends as a stop ends it once the word's directory is
//! gone. Each test asserts an order, never a duration: what the stream holds before the
//! word is given, then what comes after. A word is a place that can stand, so a fixture
//! that writes one as a relative path is refused as it is read.

use std::path::Path;

use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use yunta_adapters::{MockAdapter, MockFixture};
use yunta_core::events::ToolAction;
use yunta_core::port::{Adapter, AgentEvent};
use yunta_testkit::Hold;
use yunta_testkit_core::adapter::request;

/// The CLIs a fixture stands for: the step is the mock's own, so each holds alike.
const DIALECTS: [&str; 2] = ["claude-code", "codex"];

/// The word the session waits for between its two reads.
const WORD: &str = "read-b";

/// A session under `dialect` that reads `a` in `checkout`, waits for `word`, then reads
/// `b` and completes.
fn held_between_two_reads(dialect: &str, checkout: &Path, word: &Path) -> MockAdapter {
    let a = checkout.join("a");
    let b = checkout.join("b");
    std::fs::write(&a, "a\n").unwrap();
    std::fs::write(&b, "b\n").unwrap();
    MockAdapter::from_yaml(&format!(
        "dialect: {dialect}\nsteps:\n  - {{ type: file_call, action: read, path: \"{}\" }}\n  - {{ type: awaits_word, word: \"{}\" }}\n  - {{ type: file_call, action: read, path: \"{}\" }}\noutcome: {{ type: completed, summary: done }}\n",
        a.display(),
        word.display(),
        b.display()
    ))
    .unwrap_or_else(|error| panic!("{dialect}: {error}"))
}

/// The file `event` reads, as the stream shows it, when it reads one.
fn read(event: &AgentEvent) -> Option<&str> {
    match event {
        AgentEvent::ToolUse {
            action: ToolAction::Read,
            target,
            ..
        } => target.display(),
        _ => None,
    }
}

/// Whether `event` ends the session's stream with an outcome.
fn terminal(event: &AgentEvent) -> bool {
    matches!(
        event,
        AgentEvent::Completed { .. } | AgentEvent::Failed { .. }
    )
}

/// Takes events off `events` until one reads `file`; a stream that ends first fails the
/// test with what came.
async fn until_read(events: &mut BoxStream<'_, AgentEvent>, file: &str, dialect: &str) {
    let mut came = Vec::new();
    while let Some(event) = events.next().await {
        let reads = read(&event) == Some(file);
        came.push(event);
        if reads {
            return;
        }
    }
    panic!("{dialect}: the stream ended before `{file}` was read: {came:?}");
}

/// What the stream already holds, taken without waiting for more, and whether it ended.
fn held_now(events: &mut BoxStream<'_, AgentEvent>) -> (Vec<AgentEvent>, bool) {
    let mut held = Vec::new();
    loop {
        match events.next().now_or_never() {
            Some(Some(event)) => held.push(event),
            Some(None) => return (held, true),
            None => return (held, false),
        }
    }
}

/// Under each dialect, a session whose word is never given ends once the directory
/// holding the word is gone: it read `a`, then ends as a stop ends it, `b` never read
/// and no outcome said.
#[tokio::test]
async fn a_session_whose_word_never_comes_ends_once_its_directory_is_gone() {
    for dialect in DIALECTS {
        let checkout = tempfile::tempdir().unwrap();
        let hold = Hold::new();
        let mock = held_between_two_reads(dialect, checkout.path(), &hold.word(WORD));
        let mut session = mock
            .spawn(request(checkout.path().to_path_buf()))
            .await
            .unwrap();
        let mut events = session.events();

        until_read(&mut events, "a", dialect).await;
        drop(hold);
        let after: Vec<AgentEvent> = events.collect().await;

        assert!(
            !after.iter().any(|event| read(event) == Some("b")),
            "{dialect}: `b` was read with no word given: {after:?}"
        );
        assert!(
            !after.iter().any(terminal),
            "{dialect}: a session whose word never came said an outcome: {after:?}"
        );
    }
}

/// Under each dialect, a session held by a word stands after reading `a`, `b` unread and
/// its stream open, until the word is given; then it reads `b` and completes.
#[tokio::test]
async fn a_session_held_by_a_word_goes_on_once_the_word_is_given() {
    for dialect in DIALECTS {
        let checkout = tempfile::tempdir().unwrap();
        let hold = Hold::new();
        let mock = held_between_two_reads(dialect, checkout.path(), &hold.word(WORD));
        let mut session = mock
            .spawn(request(checkout.path().to_path_buf()))
            .await
            .unwrap();
        let mut events = session.events();

        until_read(&mut events, "a", dialect).await;
        let (held, ended) = held_now(&mut events);
        assert!(
            !ended
                && !held
                    .iter()
                    .any(|event| read(event) == Some("b") || terminal(event)),
            "{dialect}: the session went on before its word: {held:?}, ended: {ended}"
        );
        hold.give(WORD);
        let after: Vec<AgentEvent> = events.collect().await;

        let b = after
            .iter()
            .position(|event| read(event) == Some("b"))
            .unwrap_or_else(|| panic!("{dialect}: `b` never came after the word: {after:?}"));
        assert!(
            matches!(after.last(), Some(AgentEvent::Completed { result }) if result.summary == "done")
                && b < after.len() - 1,
            "{dialect}: the session did not complete after reading `b`: {after:?}"
        );
    }
}

/// Under each dialect, a word written as a relative path is refused as the fixture is
/// read, naming the step's `word` by its place and why: its directory is the empty path,
/// which never stands, so the session would end at once and a test would read a held
/// session that was never held.
#[test]
fn a_word_that_is_not_an_absolute_path_is_refused_naming_its_step() {
    for dialect in DIALECTS {
        let refused = MockFixture::parse_without_a_run(&format!(
            "dialect: {dialect}\nsteps:\n  - {{ type: note, text: a }}\n  - {{ type: awaits_word, word: {WORD} }}\noutcome: {{ type: completed, summary: done }}\n"
        ))
        .expect_err("a relative word is refused");
        let said = refused.to_string();
        assert!(
            said.starts_with("`steps[1]`: `word`") && said.contains("a word is an absolute path"),
            "{dialect}: the refusal names the step's word and why: {said}"
        );
    }
}
