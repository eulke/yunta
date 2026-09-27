//! What a run's commands run with, and how a later wake tells it
//! changed.

use yunta_core::events::{ExecutionEnvironment, RunCreatedPayload};

fn environment(shell: &str, path: &[&str]) -> ExecutionEnvironment {
    ExecutionEnvironment {
        shell: shell.to_string(),
        path: path.iter().map(|dir| dir.to_string()).collect(),
    }
}

#[test]
fn the_same_environment_has_not_drifted() {
    let born = environment("/bin/sh", &["/usr/bin", "/bin"]);
    assert_eq!(born.drift_to(&born.clone()), None);
}

#[test]
fn a_drift_names_what_the_path_gained_and_lost() {
    let born = environment("/bin/sh", &["/usr/bin", "/bin", "/opt/old"]);
    let now = environment("/bin/sh", &["/opt/tools", "/usr/bin", "/bin"]);

    let drift = born.drift_to(&now).expect("the PATH changed");

    assert_eq!(drift.gained, vec!["/opt/tools"]);
    assert_eq!(drift.lost, vec!["/opt/old"]);
    assert_eq!(
        drift.to_string(),
        "commands now run in another environment than the run was born in: PATH gained \
         /opt/tools; PATH lost /opt/old"
    );
}

#[test]
fn the_same_directories_in_another_order_are_a_drift_too() {
    let born = environment("/bin/sh", &["/usr/bin", "/opt/tools"]);
    let now = environment("/bin/sh", &["/opt/tools", "/usr/bin"]);

    let drift = born
        .drift_to(&now)
        .expect("lookups can now find another program");

    assert!(drift.reordered);
    assert!(drift.gained.is_empty() && drift.lost.is_empty());
}

#[test]
fn another_shell_is_a_drift() {
    let born = environment("/bin/sh", &["/bin"]);
    let now = environment("/usr/local/bin/sh", &["/bin"]);

    assert_eq!(
        born.drift_to(&now).and_then(|drift| drift.shell),
        Some(("/bin/sh".to_string(), "/usr/local/bin/sh".to_string()))
    );
}

#[test]
fn a_run_born_before_its_environment_was_recorded_still_reads() {
    let created: RunCreatedPayload = serde_json::from_str(
        r#"{"manifest_hash":"0000000000000000000000000000000000000000000000000000000000000000",
            "inputs":{},"mode":"default","base_branch":"main","base_commit":"deadbeef"}"#,
    )
    .expect("an older run_created");
    assert_eq!(created.environment, None);
}
