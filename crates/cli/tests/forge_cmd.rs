//! A run that would open a pull request through a forge this machine
//! cannot reach is refused before it exists, and `yunta doctor` says why.

use std::path::PathBuf;

use yunta_testkit::{init_repo, stderr, stdout, write, yunta_in};

const OPENS: &str =
    "name: opens\nnodes:\n  - { id: pr, kind: pull_request, title: \"add dark mode\" }\n";

/// A project whose config declares a forge whose token variable is set
/// nowhere.
fn a_project_with_an_unreachable_forge() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    write(
        &repo.join(".yunta/config.yaml"),
        "forge:\n  github: { repo: acme/web, token_env: YUNTA_TEST_FORGE_TOKEN_NOBODY_SETS }\n",
    );
    write(&repo.join("wf.yaml"), OPENS);
    let home = root.path().join("state");
    (root, repo, home)
}

#[test]
fn yunta_run_refuses_before_creating_a_run_when_the_forge_token_is_unset() {
    let (_root, repo, home) = a_project_with_an_unreachable_forge();

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);

    assert!(!run.status.success(), "{}", stdout(&run));
    let said = stderr(&run);
    assert!(
        said.contains("`YUNTA_TEST_FORGE_TOKEN_NOBODY_SETS`") && said.contains("is not set here"),
        "{said}"
    );
    let listed = yunta_in!(&repo, &home, &["list", "--runs"]);
    assert!(
        stdout(&listed).starts_with("no runs"),
        "{}",
        stdout(&listed)
    );
}

#[test]
fn doctor_names_an_unset_forge_token() {
    let (_root, repo, home) = a_project_with_an_unreachable_forge();
    // In the catalog, the workflow is one a person runs from here: its
    // pull request is what the token would be for.
    write(&repo.join(".yunta/workflows/opens.yaml"), OPENS);

    let doctor = yunta_in!(&repo, &home, &["doctor"]);

    assert!(!doctor.status.success());
    assert!(
        yunta_testkit::checked(&stdout(&doctor), "forge").is_some_and(|said| said.contains(
            "github acme/web — `YUNTA_TEST_FORGE_TOKEN_NOBODY_SETS`, the variable its token is in, is not set, and `opens` opens a pull request through it"
        )),
        "{}",
        stdout(&doctor)
    );
}
