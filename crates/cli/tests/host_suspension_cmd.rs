//! `yunta status` and `yunta stats` say when the host a run worked on
//! slept, and that its durations leave that time out.

use std::time::Duration;

use yunta_core::events::{EventDraft, EventPayload, HostSuspendedPayload, RunEvent};
use yunta_core::RunId;
use yunta_storage::Storage;
use yunta_testkit::{init_repo, run_id_from, stderr, stdout, write, yunta_in};

const ONE_NODE: &str = "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n";

/// A finished run whose log then records that its host slept an hour and
/// then another twenty minutes — written the way the engine writes it.
fn a_run_whose_host_slept() -> (tempfile::TempDir, String) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(&repo.join("wf.yaml"), ONE_NODE);
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "stderr: {}", stderr(&run));
    let run_id = run_id_from(&run);

    let storage = Storage::open(&home.join("yunta.db")).unwrap();
    let id = RunId::from(run_id.as_str());
    let last = storage
        .events_for_run(&id)
        .unwrap()
        .last()
        .expect("the run's log")
        .timestamp;
    for slept in [60 * 60, 20 * 60] {
        storage
            .append_at(
                &EventDraft {
                    run_id: id.clone(),
                    node_id: None,
                    payload: EventPayload::Run(RunEvent::HostSuspended(
                        HostSuspendedPayload::slept(Duration::from_secs(slept)),
                    )),
                },
                last,
            )
            .unwrap();
    }
    (root, run_id)
}

#[test]
fn status_says_how_long_the_host_slept_in_all() {
    let (root, run_id) = a_run_whose_host_slept();
    let repo = root.path().join("repo");
    let home = root.path().join("state");

    let status = yunta_in!(&repo, &home, &["status", &run_id]);

    assert!(
        stdout(&status)
            .contains("host: suspended 2 times for 1h20m in all — durations leave it out"),
        "{}",
        stdout(&status)
    );
}
