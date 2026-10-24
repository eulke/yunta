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

/// Each problem a check's page lists: what it says, read whole, the place
/// its `-->` line points at, and what its `help:` says, read whole.
fn problems(page: &str) -> Vec<(String, String, String)> {
    page.split("\n  error  ")
        .skip(1)
        .map(|problem| {
            let (said, rest) = problem.split_once("-->").unwrap_or((problem, ""));
            let place = rest.lines().next().unwrap_or_default().trim().to_string();
            let help = rest
                .split_once("help:")
                .map(|(_, help)| help.split("\n\n").next().unwrap_or_default())
                .unwrap_or_default();
            (
                yunta_testkit::unwrapped(said),
                place,
                yunta_testkit::unwrapped(help),
            )
        })
        .collect()
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
    let problems = problems(&stderr);
    for (key, instead, line) in [
        ("role", "a node names its runner with `runner:`", 12),
        ("depend_on", "did you mean `depends_on`?", 13),
    ] {
        let (_, place, help) = problems
            .iter()
            .find(|(said, _, _)| said.contains(&format!("unknown key `{key}`")))
            .unwrap_or_else(|| panic!("`{key}` is named: {stderr}"));
        assert!(
            help.contains(instead) && place.ends_with(&at(line)),
            "`{key}` is refused at line {line}, with what replaces it: {stderr}"
        );
    }
    // Where every key is listed is said once, under the first of them.
    let listed: Vec<&str> = problems
        .iter()
        .filter(|(_, _, help)| help.contains("`yunta schema workflow` lists every key"))
        .map(|(said, _, _)| said.as_str())
        .collect();
    assert!(
        listed.len() == 1 && listed[0].contains("unknown key `role`"),
        "{stderr}"
    );
}

#[test]
fn check_names_a_slip_in_a_kind_where_it_is_written_and_what_it_meant() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("slips.yaml");
    std::fs::write(
        &path,
        "name: w\nnodez: x\nnodes:\n  - id: fix\n    kind: prompy\n    prompt: p\n  - id: checks\n    kind: parallel\n    nodes:\n      - { id: lint, kind: bash, run: \"true\" }\n      - id: test\n        kind: bsh\n        run: \"true\"\n",
    )
    .unwrap();
    let out = yunta_in!(
        dir.path(),
        &dir.path().join("home"),
        &["check", path.to_str().unwrap()]
    );
    assert!(
        !out.status.success(),
        "a file with slips must not pass check"
    );
    let stderr = yunta_testkit::stderr(&out);
    let problems = problems(&stderr);
    let about = |said: &str| {
        problems
            .iter()
            .find(|(written, _, _)| written.contains(said))
            .unwrap_or_else(|| panic!("`{said}` is named: {stderr}"))
    };
    let (_, place, help) = about("the document has an unknown key `nodez`");
    assert!(
        place.ends_with("slips.yaml:2:1")
            && help == "did you mean `nodes`? · `yunta schema workflow` lists every key and kind",
        "a key at the top, with the command that lists every key and kind: {stderr}"
    );
    let (_, place, help) = about("node `fix`: unknown kind `prompy`");
    assert!(
        place.ends_with("slips.yaml:5:11") && help == "did you mean `prompt`?",
        "a kind where it is written, the command not said twice: {stderr}"
    );
    let (_, place, help) = about("node `test`: unknown kind `bsh`");
    assert!(
        place.ends_with("slips.yaml:12:15") && help == "did you mean `bash`?",
        "a child's kind at its own line: {stderr}"
    );
}
