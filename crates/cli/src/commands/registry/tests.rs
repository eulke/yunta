use super::*;

/// A fixture that is not YAML at all is refused at birth, and the refusal
/// names the file, so the person knows which fixture to fix.
#[test]
fn a_fixture_that_does_not_parse_is_refused_naming_its_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("broken.yaml");
    std::fs::write(&path, "sessions: [unclosed\n").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let error = runtime
        .block_on(read_fixture(&path))
        .expect_err("a fixture that is not YAML is refused");
    assert!(error.to_string().contains("broken.yaml"), "{error}");
}

/// The refusal offers the adapters `runners:` names that this binary
/// builds, and says so in words of its own when there are none.
#[test]
fn an_unbuilt_pin_names_what_this_binary_can_run() {
    let refused = |runnable: Vec<AdapterId>| {
        ServableError::UnbuiltPin {
            pin: AdapterId::from("codex"),
            runnable,
        }
        .to_string()
    };
    assert_eq!(
        refused(vec![AdapterId::from("claude-code")]),
        "unknown adapter `codex`; this binary can run: claude-code"
    );
    assert_eq!(
        refused(Vec::new()),
        "unknown adapter `codex`; this binary can run: (none configured: `runners:` names no adapter this build supports)"
    );
}
