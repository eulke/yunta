//! What a Claude Code session's sandbox lets its commands write and reach: the
//! roots and the temporary directories a session's commands may write, what a
//! read-only session keeps out of their reach, and the coverage each profile
//! reports for them.

use super::*;
use yunta_core::fence::Coverage;

/// The commands of a session write the roots the CLI may write, and the
/// temporary directories beside them: the sandbox its settings carry allows
/// both.
#[tokio::test]
async fn a_claude_session_lets_its_commands_write_the_roots_and_temp() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    let mut req = request(dir.path().to_path_buf());
    req.artifact_dir = Some(artifacts.clone());
    confine(
        &mut req,
        PermissionProfile::Edit,
        EditSandbox::default(),
        true,
        true,
    )
    .unwrap();
    let args = argv_for(dir.path(), req).await;

    let settings = args
        .iter()
        .position(|a| a == "--settings")
        .map(|pos| args[pos + 1].clone())
        .expect("a session is handed its settings");
    let parsed: serde_json::Value = serde_json::from_str(&settings).unwrap();
    let writable = parsed["sandbox"]["filesystem"]["allowWrite"]
        .as_array()
        .unwrap();
    assert!(
        writable.contains(&serde_json::json!(artifacts.display().to_string())),
        "the commands write the roots too: {writable:?}"
    );
    assert!(
        writable.contains(&serde_json::json!("/tmp")),
        "and the temporary directories: {writable:?}"
    );
}

/// A read-only session's commands write the temporary directories and
/// nothing of the checkout: its settings allow the first and deny the
/// working directory.
#[tokio::test]
async fn a_read_only_claude_session_writes_temp_and_nothing_of_the_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path().to_path_buf());
    confine(
        &mut req,
        PermissionProfile::ReadOnly,
        EditSandbox::default(),
        false,
        true,
    )
    .unwrap();
    let args = argv_for(dir.path(), req).await;
    let at = args
        .iter()
        .position(|arg| arg == "--settings")
        .expect("handed its settings")
        + 1;
    let settings: serde_json::Value = serde_json::from_str(&args[at]).unwrap();
    let filesystem = &settings["sandbox"]["filesystem"];
    assert!(filesystem["allowWrite"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("/tmp")));
    assert!(filesystem["denyWrite"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!(dir.path())));
}

/// Commands write the temporary directories, and an edit session's commands
/// write its checkout as well. So a read-only session's coverage names temp
/// and not its checkout, an edit session's names its checkout and temp, and
/// a full session's commands are not confined at all.
#[tokio::test]
async fn coverage_follows_what_the_sandbox_leaves_commands() {
    for profile in [
        PermissionProfile::ReadOnly,
        PermissionProfile::Edit,
        PermissionProfile::Full,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut req = request(dir.path().to_path_buf());
        confine(&mut req, profile, EditSandbox::default(), false, true).unwrap();
        let events = spawn_with(&dir, req, &[INIT_LINE, RESULT_LINE]).await;

        let AgentEvent::SessionOpened { fence, .. } = &events[0] else {
            panic!("a session opens first: {events:?}");
        };
        if profile == PermissionProfile::ReadOnly {
            assert!(
                matches!(
                    fence,
                    Some(Coverage::WidenedToRoots { roots }) if roots.contains(&PathBuf::from("/tmp"))
                ),
                "a read-only session's commands write temp: {fence:?}"
            );
        } else {
            let expected = match profile {
                PermissionProfile::Edit => Coverage::WidenedToRoots {
                    roots: [vec![dir.path().to_path_buf()], yunta_core::temp_dirs()].concat(),
                },
                _ => Coverage::ToolsOnly,
            };
            assert_eq!(fence.as_ref(), Some(&expected), "under {profile:?}");
        }
    }
}
