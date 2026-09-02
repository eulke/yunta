//! `yunta check` on a file with keys the schema does not accept names
//! every one of them, so an author fixes the file in one pass.

use std::path::PathBuf;
use std::process::Command;

fn yunta() -> Command {
    Command::new(env!("CARGO_BIN_EXE_yunta"))
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn check_names_every_unknown_key_of_a_node_with_the_key_that_replaces_it() {
    let out = yunta()
        .args(["check", fixture("typo-keys.yaml").to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "a workflow with unknown keys must not pass check"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("typo-keys.yaml"),
        "the file is named: {stderr}"
    );
    assert!(stderr.contains("`depend_on`"), "{stderr}");
    assert!(
        stderr.contains("`role`") && stderr.contains("`runner:`"),
        "{stderr}"
    );
    assert!(
        stderr.contains("`depends_on`"),
        "the valid keys are listed: {stderr}"
    );
}
