use futures::StreamExt;
use std::collections::BTreeMap;
use std::path::Path;
use yunta_core::port::session::SessionEnv;
use yunta_core::port::AgentEvent;
use yunta_core::process::signal::{liveness, Liveness};
use yunta_core::process::subprocess::{self, Launch, LineParser};
use yunta_core::AdapterId;

/// A parser that fails on the first line it is given, so its reader ends
/// after the session's output has begun.
struct FailsOnFirstLine;

impl LineParser for FailsOnFirstLine {
    fn parse(&mut self, _line: &str) -> Vec<AgentEvent> {
        panic!("the parser fails on its first line")
    }
}

#[tokio::test]
async fn a_failed_reader_does_not_make_the_kill_of_a_closed_group_fail() {
    static ADAPTER: AdapterId = AdapterId::from_static("probe");
    let dir = tempfile::tempdir().unwrap();
    let env = SessionEnv {
        plain: BTreeMap::new(),
        secret: BTreeMap::new(),
    };
    let mut session = subprocess::open(Launch {
        adapter: &ADAPTER,
        binary: Path::new("/bin/sh"),
        args: vec!["-c".into(), "echo first; sleep 30".into()],
        cwd: dir.path(),
        env: &env,
        prompt: "",
        parser: Box::new(FailsOnFirstLine),
        held: None,
    })
    .await
    .unwrap();
    let pgid = session
        .pgid()
        .expect("a session that just opened names its process group");
    // The stream ends when the reader drops its sender, which a panicked reader does.
    session.events().collect::<Vec<AgentEvent>>().await;
    session
        .kill()
        .await
        .expect("the group closed, so the kill answers ok");
    assert_eq!(liveness(pgid), Liveness::Dead);
}
