//! `yunta doctor` validating an installed pack's `requires:` against the
//! local config — end to end against the real
//! compiled binary.

use std::path::Path;

use yunta_testkit::{git, init_repo, stderr, stdout, yunta_in};

fn write_pack_with_requires(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         requires:\n  \
           runners: [{ name: reviewer }]\n  \
           mcp_servers: [internal-docs]\n  \
           commands: [this-binary-almost-certainly-does-not-exist-anywhere]\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        "name: review\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "v1"]);
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack_with_requires(&upstream);

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let home = root.path().join("state");
    (root, upstream, home)
}

#[test]
fn doctor_flags_a_pack_whose_requires_the_local_config_cannot_satisfy() {
    let (root, upstream, home) = setup();
    let repo = root.path().join("repo");

    let add_out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let doctor_out = yunta_in!(&repo, &home, &["doctor"]);
    assert!(!doctor_out.status.success());
    let text = stdout(&doctor_out);
    assert!(text.contains("pack acme/review-pack requires"), "{text}");
    assert!(text.contains("runner `reviewer`"), "{text}");
    assert!(text.contains("mcp_server `internal-docs`"), "{text}");
    assert!(
        text.contains("command `this-binary-almost-certainly-does-not-exist-anywhere`"),
        "{text}"
    );
}

#[test]
fn doctor_is_silent_about_a_pack_with_no_unmet_requires() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    std::fs::create_dir_all(upstream.join("workflows")).unwrap();
    std::fs::write(
        upstream.join("pack.yaml"),
        "name: review-pack\npublisher: acme\nversion: 1.0.0\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        upstream.join("workflows/review.yaml"),
        "name: review\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
    git(&upstream, &["add", "."]);
    git(&upstream, &["commit", "-q", "-m", "v1"]);

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let add_out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let doctor_out = yunta_in!(&repo, &home, &["doctor"]);
    assert!(doctor_out.status.success(), "{}", stderr(&doctor_out));
    assert!(!stdout(&doctor_out).contains("requires"));
}
