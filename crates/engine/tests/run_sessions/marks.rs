//! Every session a run opens carries a mark of its own that names its run,
//! so what the session's CLI starts can be told from what any other session
//! started.

use yunta_core::process::mark::SessionMark;
use yunta_testkit::Bench;

const TWO_SESSIONS: &str = r#"
name: two-sessions
nodes:
  - { id: first, kind: prompt, runner: executor, prompt: "One." }
  - { id: second, kind: prompt, runner: executor, prompt: "Two.", depends_on: [first] }
"#;

const BOTH_COMPLETE: &str = "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: one }\n  - outcome: { type: completed, summary: two }\n";

/// Two sessions of one run get marks that differ, and the marks of that run
/// hold both. A session of another run gets a mark the first run's marks do
/// not hold.
#[tokio::test]
async fn each_session_carries_a_mark_of_its_run_and_its_own() {
    let bench = Bench::new();
    bench.run(TWO_SESSIONS, BOTH_COMPLETE).await;
    let other = Bench::with_run_id("run-test-2");
    other.run(TWO_SESSIONS, BOTH_COMPLETE).await;

    let marks: Vec<SessionMark> = bench
        .mock()
        .requests_seen()
        .into_iter()
        .map(|request| request.mark)
        .collect();
    let elsewhere = other.mock().requests_seen().remove(0).mark;

    assert_eq!(marks.len(), 2);
    assert_ne!(marks[0], marks[1]);
    let of_run = SessionMark::of_run(&bench.run_id);
    assert!(
        marks.iter().all(|mark| of_run.hold(mark.as_str())),
        "{marks:?}"
    );
    assert!(!of_run.hold(elsewhere.as_str()), "{elsewhere:?}");
}
