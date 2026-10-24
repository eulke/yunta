//! What the probe a run makes before it spends anything says of a CLI it
//! cannot find.

use std::path::Path;

use yunta_core::port::ProbeReport;
use yunta_core::process::subprocess::probe_version;

#[tokio::test]
async fn a_cli_on_no_directory_of_the_path_is_said_to_be_missing() {
    let report = probe_version(Path::new("yunta-probe-names-no-such-cli")).await;
    assert_eq!(
        report,
        ProbeReport::Unhealthy {
            diagnostic: "`yunta-probe-names-no-such-cli` is not on PATH".to_string()
        }
    );
}

#[tokio::test]
async fn a_path_that_names_no_file_is_said_to_not_exist() {
    let report = probe_version(Path::new("/nonexistent/yunta-probe/cli")).await;
    assert_eq!(
        report,
        ProbeReport::Unhealthy {
            diagnostic: "`/nonexistent/yunta-probe/cli` does not exist".to_string()
        }
    );
}

#[tokio::test]
async fn a_file_that_is_there_and_cannot_start_is_not_called_missing() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("cli");
    std::fs::write(&script, "#!/nonexistent/interpreter\n").unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let ProbeReport::Unhealthy { diagnostic } = probe_version(&script).await else {
        panic!("a CLI that cannot start is not healthy");
    };
    assert!(
        diagnostic.starts_with(&format!("`{}` could not be run: ", script.display())),
        "{diagnostic}"
    );
}
