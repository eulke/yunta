//! `yunta check` on a file with keys the schema does not accept names
//! every one of them, so an author fixes the file in one pass.

use std::path::PathBuf;

use yunta_testkit::yunta_in;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn check_names_every_unknown_key_of_a_node_with_the_key_that_replaces_it() {
    // The file names itself: the command runs from an empty directory
    // with a home of its own, so nothing the developer's machine holds
    // — a project config beside the invocation, a `~/.yunta` — decides
    // what the parser sees.
    let dir = tempfile::tempdir().unwrap();
    let path = fixture("typo-keys.yaml");
    let out = yunta_in!(
        dir.path(),
        &dir.path().join("home"),
        &["check", path.to_str().unwrap()]
    );
    assert!(
        !out.status.success(),
        "a workflow with unknown keys must not pass check"
    );
    let stderr = yunta_testkit::stderr(&out);
    assert!(
        stderr.contains("typo-keys.yaml"),
        "the file is named: {stderr}"
    );
    let unknown_clause = stderr
        .split_once("unknown key(s) ")
        .and_then(|(_, rest)| rest.split_once("; valid keys:"))
        .map(|(clause, _)| clause);
    assert_eq!(
        unknown_clause,
        Some("`role`, `depend_on` for a `prompt` node"),
        "both mistyped keys are named together so the author fixes them in one pass: {stderr}"
    );
    assert!(
        stderr.contains("`role`") && stderr.contains("`runner:`"),
        "{stderr}"
    );
    assert!(
        stderr.contains("`depends_on`"),
        "the valid keys are listed: {stderr}"
    );
}
