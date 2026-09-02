use yunta_core::ConfigLayer;
use yunta_engine::{resolve_runner, RunnerError};

fn config(yaml: &str) -> ConfigLayer {
    serde_yaml::from_str(yaml).unwrap()
}

const CONFIG: &str = r#"
runners:
  executor:
    - { adapter: claude-code, model: claude-sonnet-4-6 }
    - { adapter: codex, model: gpt-5-codex }
  reviewer:
    - { adapter: claude-code, model: claude-sonnet-4-6, agent: benito }
"#;

#[test]
fn the_first_available_candidate_wins_in_declared_order() {
    let resolved = resolve_runner("executor", &config(CONFIG), &|_| true, None).unwrap();
    assert_eq!(resolved.chosen.adapter, "claude-code");
    assert!(resolved.discarded.is_empty());
}

#[test]
fn an_unavailable_adapter_is_discarded_with_a_reason_not_skipped_silently() {
    let resolved = resolve_runner(
        "executor",
        &config(CONFIG),
        &|adapter| adapter == "codex",
        None,
    )
    .unwrap();
    assert_eq!(resolved.chosen.adapter, "codex");
    assert_eq!(resolved.discarded.len(), 1);
    assert_eq!(resolved.discarded[0].candidate.adapter, "claude-code");
    assert!(!resolved.discarded[0].reason.is_empty());
}

#[test]
fn a_role_with_no_available_candidate_is_an_error_naming_what_was_tried() {
    let err = resolve_runner("executor", &config(CONFIG), &|_| false, None).unwrap_err();
    match err {
        RunnerError::NoCandidateAvailable { role, tried } => {
            assert_eq!(role, "executor");
            assert_eq!(tried, ["claude-code", "codex"]);
        }
        other => panic!("expected NoCandidateAvailable, got {other}"),
    }
}

#[test]
fn an_unknown_role_is_an_error() {
    let err = resolve_runner("ghost", &config(CONFIG), &|_| true, None).unwrap_err();
    assert!(matches!(err, RunnerError::UnknownRole { .. }));
}

#[test]
fn the_chosen_candidate_carries_its_agent_through() {
    let resolved = resolve_runner("reviewer", &config(CONFIG), &|_| true, None).unwrap();
    assert_eq!(resolved.chosen.agent.as_deref(), Some("benito"));
}

#[test]
fn an_adapter_override_wins_over_declaration_order_and_records_the_discards() {
    let override_id = yunta_core::AdapterId::from("codex");
    let resolved =
        resolve_runner("executor", &config(CONFIG), &|_| true, Some(&override_id)).unwrap();
    assert_eq!(resolved.chosen.adapter, "codex");
    assert_eq!(resolved.discarded.len(), 1);
    assert_eq!(resolved.discarded[0].candidate.adapter, "claude-code");
    assert!(
        resolved.discarded[0].reason.contains("--adapter"),
        "the reason names the override: {}",
        resolved.discarded[0].reason
    );
}

#[test]
fn an_adapter_override_with_no_candidate_on_it_is_an_error_naming_what_was_tried() {
    let override_id = yunta_core::AdapterId::from("codex");
    let err =
        resolve_runner("reviewer", &config(CONFIG), &|_| true, Some(&override_id)).unwrap_err();
    match err {
        RunnerError::OverrideHasNoCandidate {
            role,
            adapter,
            candidates,
        } => {
            assert_eq!(role, "reviewer");
            assert_eq!(adapter, "codex");
            assert_eq!(candidates, ["claude-code"]);
        }
        other => panic!("unexpected: {other:?}"),
    }
}
