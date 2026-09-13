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

/// A workflow whose one node leaves an artifact behind, so the run's log
/// names an object `verify` can check.
const ONE_ARTIFACT: &str = "name: one-artifact
nodes:
  - id: only
    kind: bash
    run: \"echo the-bytes > {{node.artifacts}}/report.md\"
    artifacts:
      produces: [report.md]
";

#[test]
fn a_corrupt_object_is_reported_beside_an_intact_chain() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(&repo.join("wf.yaml"), ONE_ARTIFACT);
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "stderr: {}", stderr(&run));
    let run_id = run_id_from(&run);

    // Out of the box both guarantees hold and the command exits 0.
    let intact = yunta_in!(&repo, &home, &["verify", &run_id]);
    assert!(intact.status.success(), "stderr: {}", stderr(&intact));
    let text = stdout(&intact);
    assert!(text.contains("chain intact"), "got: {text}");
    assert!(
        text.contains("objects intact") && text.contains("1 artifact"),
        "the objects are their own report: {text}"
    );

    // Replace the bytes of the run's one object. The chain covers the
    // log, not the store, so it stays intact — and the two are reported
    // apart.
    let objects = home.join("runs").join(&run_id).join("objects");
    let object = std::fs::read_dir(&objects)
        .unwrap()
        .next()
        .expect("the run stored its artifact")
        .unwrap()
        .path();
    std::fs::write(&object, b"not what the run accepted").unwrap();

    let broken = yunta_in!(&repo, &home, &["verify", &run_id]);
    assert!(
        !broken.status.success(),
        "an object that is not its own bytes must exit non-zero: {}",
        stderr(&broken)
    );
    assert!(
        stdout(&broken).contains("chain intact"),
        "a corrupt object does not break the chain: {}",
        stdout(&broken)
    );
    let reported = stderr(&broken);
    assert!(
        reported.contains("objects BROKEN") && reported.contains("report.md"),
        "the failing artifact is named: {reported}"
    );
}
