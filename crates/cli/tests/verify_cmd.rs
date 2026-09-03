//! `yunta verify <run_id>`: the CLI face of the event-log hash chain. An
//! intact chain reports intact and exits 0; a chain broken behind the
//! store's back exits non-zero and names the exact seq the break begins at,
//! so a person knows from where on the log can no longer be trusted.

use yunta_core::{RunId, Seq};
use yunta_storage::Storage;
use yunta_testkit::{init_repo, run_id_from, stderr, stdout, write, yunta_in};

const ONE_NODE: &str = "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n";

#[test]
fn broken_chain_exits_nonzero_naming_the_seq() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(&repo.join("wf.yaml"), ONE_NODE);
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "stderr: {}", stderr(&run));
    let run_id = run_id_from(&run);

    // Out of the box the chain verifies and the command exits 0.
    let intact = yunta_in!(&repo, &home, &["verify", &run_id]);
    assert!(intact.status.success());
    assert!(
        stdout(&intact).contains("intact"),
        "got: {}",
        stdout(&intact)
    );

    // Corrupt the second event behind the store's back — the disk-level
    // tamper the chain exists to catch — and verify must refuse it.
    let storage = Storage::open(&home.join("yunta.db")).unwrap();
    storage
        .corrupt_event_payload(&RunId::from(run_id.as_str()), Seq::FIRST.next())
        .unwrap();
    drop(storage);

    let broken = yunta_in!(&repo, &home, &["verify", &run_id]);
    assert!(
        !broken.status.success(),
        "a broken chain must exit non-zero: {}",
        stderr(&broken)
    );
    assert!(
        stderr(&broken).contains("BROKEN at seq 2"),
        "the failing seq must be named: {}",
        stderr(&broken)
    );
}
