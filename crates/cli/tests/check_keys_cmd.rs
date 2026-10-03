//! `yunta check` on a file with keys the schema does not accept names
//! every one of them where it is written, beside every other problem the
//! file has, so an author fixes the file in one pass.

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
    // Each key is refused where it is written, with what the author
    // reaches for instead: a retired key with the key that replaced it,
    // a typo with the key it is one slip from.
    let at = |line: u32| format!("typo-keys.yaml:{line}:5");
    for (key, instead, line) in [
        ("role", "a node names its runner with `runner:`", 12),
        ("depend_on", "did you mean `depends_on`?", 13),
    ] {
        let refusal = stderr
            .lines()
            .position(|said| said.contains(&format!("unknown key `{key}`")))
            .unwrap_or_else(|| panic!("`{key}` is named: {stderr}"));
        let said: Vec<&str> = stderr.lines().skip(refusal).take(2).collect();
        assert!(
            said[0].contains(instead) && said[1].ends_with(&at(line)),
            "`{key}` is refused at line {line}, with what replaces it: {stderr}"
        );
    }
    assert!(
        stderr.contains("`depends_on`"),
        "the valid keys are listed: {stderr}"
    );
}
