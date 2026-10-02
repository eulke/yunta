//! What a spec is held to when it is handed over: its tests run the files
//! it writes.

use yunta_core::SpecFile;

/// A spec with the shape of one that tested nothing: each task's file is
/// `assert!(true)` that no command runs, and each test is the plan's own
/// criterion, which runs only what the task writes.
const HOLLOW: &str = r##"
specs:
  - task: cli
    files:
      - { path: tests/scope_spec.rs, content: "#[test] fn holds() { assert!(true); }\n" }
    tests:
      - { cmd: "cargo test --test pack_cmd && grep -q 'scope_case' tests/pack_cmd.rs", proves: p }
  - task: engine
    files:
      - { path: tests/resolution_spec.rs, content: "#[test] fn holds() { assert!(true); }\n" }
    tests:
      - { cmd: "cargo test --lib && grep -q 'resolution_case' src/catalog.rs", proves: p }
"##;

fn codes(spec: &SpecFile) -> Vec<String> {
    spec.untested()
        .iter()
        .map(|diagnostic| diagnostic.problem.code().to_string())
        .collect()
}

#[test]
fn a_spec_whose_tests_run_none_of_its_files_is_refused() {
    let spec: SpecFile = yunta_core::yaml::parse(HOLLOW).unwrap();
    assert_eq!(
        codes(&spec),
        [
            "unrun-spec-file",
            "spec-test-runs-no-spec-file",
            "unrun-spec-file",
            "spec-test-runs-no-spec-file"
        ]
    );
}

#[test]
fn a_spec_whose_tests_run_its_files_or_that_writes_none_holds() {
    let runs: SpecFile = yunta_core::yaml::parse(
        r##"
specs:
  - task: cli
    files: [{ path: tests/global_pack_cmd.rs, content: "#[test] fn t() {}" }]
    tests: [{ cmd: "cargo test --test global_pack_cmd", proves: p }]
  - task: engine
    tests: [{ cmd: "yunta pack list --global | grep -q acme", proves: p }]
"##,
    )
    .unwrap();
    assert!(codes(&runs).is_empty(), "{:?}", codes(&runs));
}
