use futures::StreamExt;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
use yunta_core::port::session::SessionEnv;
use yunta_core::port::AgentEvent;
use yunta_core::process::subprocess::{self, Launch, LineParser};
use yunta_core::AdapterId;

#[tokio::test]
async fn a_launch_value_that_is_not_declared_stays_out_of_the_stderr_tail() {
    static ADAPTER: AdapterId = AdapterId::from_static("probe");
    struct Quiet;
    impl LineParser for Quiet {
        fn parse(&mut self, _line: &str) -> Vec<AgentEvent> {
            Vec::new()
        }
    }
    let dir = tempfile::tempdir().unwrap();
    // The stop-hook cap travels in the same launch environment as the secrets.
    let env = SessionEnv {
        plain: BTreeMap::from([("CAP".to_string(), "0".to_string())]),
        secret: BTreeMap::new(),
    };
    let mut session = subprocess::open(Launch {
        adapter: &ADAPTER,
        binary: Path::new("/bin/sh"),
        args: vec!["-c".into(), "echo 'retry in 10 seconds' >&2".into()],
        cwd: dir.path(),
        env: &env,
        prompt: "",
        parser: Box::new(Quiet),
        held: None,
    })
    .await
    .unwrap();
    // The child writes its line and leaves on its own. Draining its events
    // waits for its stdout to close, which is when it leaves, so the session
    // is asked for its exit only after the line was written.
    session.allow_exit(Duration::from_secs(10)).await;
    let _: Vec<AgentEvent> = session.events().collect().await;
    let exit = session
        .exit()
        .await
        .unwrap()
        .expect("the child exited on its own");
    assert_eq!(exit.stderr_tail, ["retry in 10 seconds"]);
}
