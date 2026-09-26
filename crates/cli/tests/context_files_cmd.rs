//! A `files:` path the run would not find is said before the first token
//! — by `yunta run`, `yunta check` and `yunta doctor` — end to end
//! against the real compiled binary.

use std::path::Path;

use yunta_testkit::{git, init_repo, stderr, stdout, yunta_in, Checkout, MOCK_CONFIG};

/// One prompt node that reads a file no commit of these repositories has.
const READS_ARCHITECTURE: &str = r#"
name: reads-architecture
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Plan."
    context:
      - files: ["docs/architecture.md"]
"#;

const ONE_SESSION: &str = "sessions:\n  - outcome: { type: completed, summary: planned }\n";

#[test]
fn run_warns_before_the_first_token_and_its_document_carries_the_warning() {
    let checkout = Checkout::new()
        .config(MOCK_CONFIG)
        .workflow("reads", READS_ARCHITECTURE)
        .file("fixture.yaml", ONE_SESSION)
        .committed();

    let run = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &[
            "run",
            "reads.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml",
            "--json"
        ]
    );

    let said = stderr(&run);
    assert!(
        said.contains("warning: node `plan` reads `docs/architecture.md`"),
        "the person watching reads it before anything is spent: {said}"
    );
    let document: serde_json::Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("run --json emits JSON: {e}: {}", stdout(&run)));
    let warnings = document["context_warnings"]
        .as_array()
        .unwrap_or_else(|| panic!("the document carries the warning: {document:#}"));
    assert_eq!(warnings.len(), 1, "{document:#}");
    assert!(
        warnings[0]
            .as_str()
            .is_some_and(|warning| warning.contains("does not hold")),
        "{document:#}"
    );
}

#[test]
fn check_names_a_file_the_commit_does_not_hold_without_refusing() {
    let checkout = Checkout::new()
        .config(MOCK_CONFIG)
        .workflow("reads", READS_ARCHITECTURE)
        .committed();

    let check = yunta_in!(&checkout.repo, &checkout.home, &["check", "reads.yaml"]);

    assert!(
        check.status.success(),
        "a warning, never a refusal: {}",
        stderr(&check)
    );
    assert!(
        stderr(&check).contains("node `plan` reads `docs/architecture.md`"),
        "{}",
        stderr(&check)
    );
}

#[test]
fn check_says_a_file_in_the_checkout_is_not_committed() {
    let checkout = Checkout::new()
        .workflow("reads", READS_ARCHITECTURE)
        .committed()
        .file("docs/architecture.md", "# Architecture\n");

    let check = yunta_in!(&checkout.repo, &checkout.home, &["check", "reads.yaml"]);

    assert!(
        stderr(&check).contains("which is in your checkout but not committed"),
        "{}",
        stderr(&check)
    );
}

fn write_pack(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: reads-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/reads.yaml]\n",
    )
    .unwrap();
    std::fs::write(dir.join("workflows/reads.yaml"), READS_ARCHITECTURE).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "v1"]);
}

#[test]
fn doctor_names_a_file_an_installed_pack_reads_that_the_repository_lacks() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack(&upstream);
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let add = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add.status.success(), "{}", stderr(&add));

    let doctor = yunta_in!(&repo, &home, &["doctor"]);
    assert!(!doctor.status.success(), "a missing file is a gap");
    let text = stdout(&doctor);
    assert!(
        text.lines().any(|line| line
            .starts_with("pack acme/reads-pack: node `plan` reads `docs/architecture.md`")),
        "{text}"
    );
}
