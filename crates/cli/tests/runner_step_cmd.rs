//! A project that declares no runner is told how to declare one, by
//! `init`, `check` and `doctor` alike, naming the adapter CLIs this
//! machine answers for — and `doctor` fails for it only when a workflow
//! here would stop on it.

use std::path::{Path, PathBuf};

use yunta_testkit::{stderr, stdout, yunta_at, Checkout};

/// A `PATH` on which the one adapter CLI is the scripted `claude`, so
/// what a test reads does not depend on the CLIs the machine running the
/// suite has installed.
fn only_claude(root: &Path) -> String {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(yunta_testkit_core::stubs::claude_code(), bin.join("claude"))
        .unwrap();
    format!("{}:/usr/bin:/bin", bin.display())
}

/// A checkout under `root` with [`only_claude`] as its `PATH`.
fn project(root: &Path) -> Checkout {
    let path = only_claude(root);
    Checkout::under(root).with_stubs(vec![("PATH".to_string(), path)])
}

const AGENT_WORKFLOW: &str =
    "name: wf\nnodes:\n  - id: plan\n    kind: prompt\n    prompt: plan it\n";

const SNIPPET_LINE: &str = "- { adapter: claude-code, model: <model> }";

fn markers() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_path_buf();
    (root, path)
}

#[test]
fn init_ends_with_the_runner_step() {
    let (_keep, root) = markers();
    let checkout = project(&root);
    let init = yunta_at!(&checkout, &["init"]);
    assert!(init.status.success(), "{}", stderr(&init));
    let said = stdout(&init);
    assert!(
        said.contains("this machine answers for `claude-code`")
            && said.contains(SNIPPET_LINE)
            && said.contains("      runner: implementer"),
        "{said}"
    );
}

#[test]
fn check_proposes_the_runner_a_node_lacks_on_an_adapter_this_machine_answers_for() {
    let (_keep, root) = markers();
    let checkout = project(&root).file("wf.yaml", AGENT_WORKFLOW);
    let check = yunta_at!(&checkout, &["check", "wf.yaml"]);
    assert!(!check.status.success());
    let said = stderr(&check);
    assert!(
        said.contains("declare a runner in .yunta/config.yaml")
            && said.contains(SNIPPET_LINE)
            && said.contains("runner: implementer"),
        "{said}"
    );
}

#[test]
fn doctor_names_the_missing_runner_with_a_snippet_naming_detected_adapters() {
    let (_keep, root) = markers();
    let checkout = project(&root);
    let doctor = yunta_at!(&checkout, &["doctor"]);
    assert!(
        doctor.status.success(),
        "no workflow here needs a runner, so it is a caution: {}{}",
        stdout(&doctor),
        stderr(&doctor)
    );
    assert!(
        stderr(&doctor).contains("warning: runners: none declared"),
        "{}",
        stderr(&doctor)
    );
    assert!(
        stdout(&doctor).contains(SNIPPET_LINE),
        "{}",
        stdout(&doctor)
    );
}

#[test]
fn doctor_fails_when_a_workflow_needs_a_runner_none_declares() {
    let (_keep, root) = markers();
    let checkout = project(&root).file(".yunta/workflows/wf.yaml", AGENT_WORKFLOW);
    let doctor = yunta_at!(&checkout, &["doctor"]);
    assert!(!doctor.status.success(), "{}", stdout(&doctor));
    assert!(
        stdout(&doctor).contains("runners: none declared, and `wf` needs one"),
        "{}",
        stdout(&doctor)
    );
}

const FORGE: &str =
    "forge:\n  github:\n    repo: acme/demo\n    token_env: YUNTA_TEST_ABSENT_TOKEN\n";

const OPENS_A_PR: &str =
    "name: ship\nnodes:\n  - id: pr\n    kind: pull_request\n    title: Ship it\n";

#[test]
fn doctor_only_cautions_about_an_unset_forge_token_when_no_workflow_opens_a_pull_request() {
    let (_keep, root) = markers();
    let quiet = project(&root.join("quiet")).config(FORGE);
    let doctor = yunta_at!(&quiet, &["doctor"]);
    assert!(
        doctor.status.success(),
        "nothing here opens a pull request: {}{}",
        stdout(&doctor),
        stderr(&doctor)
    );
    assert!(
        stderr(&doctor)
            .contains("`YUNTA_TEST_ABSENT_TOKEN`, the variable its token is in, is not set"),
        "{}",
        stderr(&doctor)
    );

    let shipping = project(&root.join("shipping"))
        .config(FORGE)
        .file(".yunta/workflows/ship.yaml", OPENS_A_PR);
    let doctor = yunta_at!(&shipping, &["doctor"]);
    assert!(!doctor.status.success(), "{}", stdout(&doctor));
    assert!(
        stdout(&doctor).contains("`ship` opens a pull request through it"),
        "{}",
        stdout(&doctor)
    );
}
