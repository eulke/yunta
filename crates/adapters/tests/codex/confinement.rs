//! What a Codex session's profile lets it write and reach: the version floor its
//! probe holds the CLI to, the directories a session's commands may write, and
//! the refusal of a confinement the CLI cannot close.

use super::*;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

use yunta_core::port::ProbeReport;
use yunta_core::{AdapterError, Unbuildable};

/// A CLI older than the lowest version the permissions profile was checked
/// against is unhealthy at probe, before any session opens.
#[tokio::test]
async fn a_codex_older_than_the_profile_floor_is_unhealthy_at_probe() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("codex");
    tokio::fs::write(&binary, "#!/bin/sh\necho codex-cli 0.156.0\n")
        .await
        .unwrap();
    tokio::fs::set_permissions(&binary, Permissions::from_mode(0o755))
        .await
        .unwrap();
    let adapter = CodexAdapter::new(&AdapterSettings {
        adapter_settings: None,
        binary: Some(binary),
    });
    assert!(matches!(
        adapter.probe().await.unwrap(),
        ProbeReport::Unhealthy { .. }
    ));
}

/// An unconfined edit session has no profile to close its network in, so the
/// adapter refuses a confinement that closes it, before the CLI starts.
#[tokio::test]
async fn an_unconfined_edit_session_that_closes_its_network_is_refused() {
    let settings = serde_json::json!({ "sandbox": "danger-full-access" });
    let adapter = CodexAdapter::new(&AdapterSettings {
        adapter_settings: settings.as_object().cloned(),
        binary: Some(stub_path()),
    });
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path().to_path_buf());
    confine(
        &mut req,
        PermissionProfile::Edit,
        adapter.edit_sandbox(),
        false,
        true,
    )
    .unwrap();
    // `Confinement::of` refuses a closed network on an unconfined session, so
    // the network is closed once the confinement is resolved: the adapter is
    // the one that must refuse it.
    req.confinement.network = false;
    let Err(refusal) = adapter.spawn(req).await else {
        panic!("an unconfined session with its network closed opened");
    };
    assert!(
        matches!(
            refusal,
            AdapterError::FenceUnbuildable {
                source: Unbuildable::NetworkUnclosable,
                ..
            }
        ),
        "{refusal:?}"
    );
}

#[tokio::test]
async fn a_codex_session_keeps_its_artifact_dir_writable_beside_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let args_file = dir.path().join("args.txt");

    let mut req = request(dir.path().to_path_buf());
    req.artifact_dir = Some(artifacts.clone());
    confine(
        &mut req,
        PermissionProfile::Edit,
        EditSandbox::default(),
        false,
        true,
    )
    .unwrap();
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;
    let args = std::fs::read_to_string(&args_file).unwrap();

    // The table names the artifact directory writable beside the checkout.
    // The expectation spells the key out rather than deriving it the way
    // the adapter does, so a wrong rendering cannot agree with itself.
    let filesystem = filesystem_of(&args);
    assert_eq!(
        filesystem[artifacts.display().to_string().as_str()],
        toml::Value::String("write".to_string()),
        "the artifact directory is writable beside the checkout: {args}"
    );
}

/// A read-only session's profile closes the network and names no workspace
/// to write, and it keeps the temporary directories and the artifact
/// directory writable.
#[tokio::test]
async fn a_read_only_codex_session_keeps_network_closed_and_temp_writable() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);
    let mut req = request(dir.path().to_path_buf());
    req.artifact_dir = Some(dir.path().join("run/artifacts"));
    confine(
        &mut req,
        PermissionProfile::ReadOnly,
        EditSandbox::default(),
        false,
        true,
    )
    .unwrap();
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let _ = drain(adapter().spawn(req).await.unwrap()).await;

    let args = tokio::fs::read_to_string(&args_file).await.unwrap();
    assert!(
        args.contains("permissions.yunta-session.network.enabled=false")
            && args.contains(":tmpdir")
            && args.contains(":slash_tmp")
            && !args.contains("workspace-write"),
        "{args}"
    );
}

/// The run's git register is a `read` entry of the session's profile
/// wherever it lies: a nested entry wins over the temporary directories'
/// grant, so a register under them takes no commit but the engine's.
#[tokio::test]
async fn the_runs_git_register_is_read_only_to_the_commands_even_under_temp() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);
    let register = std::env::temp_dir().join("repo/.git");
    let mut req = request(dir.path().to_path_buf());
    confine(
        &mut req,
        PermissionProfile::Edit,
        EditSandbox::default(),
        false,
        true,
    )
    .unwrap();
    req.confinement.git_register = Some(register.clone());
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let _ = drain(adapter().spawn(req).await.unwrap()).await;

    let args = tokio::fs::read_to_string(&args_file).await.unwrap();
    let entry = args
        .split(&format!("\"{}\"", register.display()))
        .nth(1)
        .unwrap_or_else(|| panic!("the register has an entry: {args}"));
    assert!(
        entry
            .trim_start()
            .trim_start_matches('=')
            .trim_start()
            .starts_with("\"read\""),
        "{args}"
    );
}

/// A read-only sandbox from the settings leaves the commands of an edit
/// session no checkout: they write their artifact directory and the
/// temporary ones, and the session reports exactly those roots.
#[tokio::test]
async fn a_read_only_sandbox_from_the_settings_leaves_the_commands_no_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    let mut extra = serde_json::Map::new();
    extra.insert("sandbox".to_string(), serde_json::Value::from("read-only"));
    let adapter = CodexAdapter::new(&AdapterSettings {
        adapter_settings: Some(extra),
        binary: Some(stub_path()),
    });
    let mut req = request(dir.path().to_path_buf());
    req.artifact_dir = Some(artifacts.clone());
    confine(
        &mut req,
        PermissionProfile::Edit,
        adapter.edit_sandbox(),
        false,
        true,
    )
    .unwrap();
    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let events = drain(adapter.spawn(req).await.unwrap()).await;

    let AgentEvent::SessionOpened { fence, .. } = &events[0] else {
        panic!("a session opens first: {events:?}");
    };
    assert_eq!(
        fence.as_ref(),
        Some(&yunta_core::fence::Coverage::WidenedToRoots {
            roots: [vec![artifacts], yunta_core::temp_dirs()].concat(),
        })
    );
}
