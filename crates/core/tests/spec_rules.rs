//! The rules a spec document has to satisfy once it is readable, each
//! asserted by the code it reports: one spec per task, a test in every
//! spec that says what it proves, and every file named once, inside the
//! repository.

use yunta_core::shape::Document;
use yunta_core::{Spec, SpecFile, SpecTest, TestFile};

fn spec(task: &str, files: &[&str], proves: &[&str]) -> Spec {
    Spec {
        task: task.into(),
        files: files
            .iter()
            .map(|path| TestFile {
                path: path.to_string(),
                content: "exit 1\n".to_string(),
            })
            .collect(),
        tests: proves
            .iter()
            .map(|proves| SpecTest {
                cmd: "sh tests/theme.sh".to_string(),
                proves: proves.to_string(),
            })
            .collect(),
    }
}

/// The stable name of every rule `specs` broke, in the order reported.
fn broken(specs: Vec<Spec>) -> Vec<String> {
    SpecFile { specs }
        .check()
        .iter()
        .map(|diagnostic| diagnostic.code().as_str().to_string())
        .collect()
}

#[test]
fn a_spec_that_holds_its_task_to_explained_tests_in_its_own_files_breaks_nothing() {
    assert!(broken(vec![
        spec("dark", &["tests/theme.sh"], &["the switch turns dark on"]),
        spec(
            "light",
            &["./tests/light.sh"],
            &["a new user starts in light"]
        ),
    ])
    .is_empty());
}

#[test]
fn a_task_given_two_specs_is_refused() {
    assert_eq!(
        broken(vec![
            spec("dark", &["tests/a.sh"], &["one"]),
            spec("dark", &["tests/b.sh"], &["two"]),
        ]),
        ["duplicate-spec"]
    );
}

#[test]
fn a_spec_without_a_test_or_with_one_that_proves_nothing_is_refused() {
    assert_eq!(
        broken(vec![spec("dark", &["tests/a.sh"], &[])]),
        ["no-spec-test"]
    );
    assert_eq!(
        broken(vec![spec("dark", &["tests/a.sh"], &["  "])]),
        ["unexplained-test"]
    );
}

#[test]
fn a_file_outside_the_repository_or_inside_git_is_refused() {
    for path in [
        "/etc/passwd",
        "../elsewhere.sh",
        ".git/hooks/pre-commit",
        "./.git/config",
        ".",
    ] {
        assert_eq!(
            broken(vec![spec("dark", &[path], &["it holds"])]),
            ["test-file-escapes"],
            "{path}"
        );
    }
}

#[test]
fn a_file_two_tests_name_is_refused() {
    assert_eq!(
        broken(vec![
            spec("dark", &["tests/theme.sh"], &["one"]),
            spec("light", &["tests/theme.sh"], &["two"]),
        ]),
        ["duplicate-test-file"]
    );
}
