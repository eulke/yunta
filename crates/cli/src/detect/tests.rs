use surco::components::Fix;
use yunta_core::{AdapterId, CommandName, ConfigKey, NodeId, RunnerName};
use yunta_engine::CheckError;

use super::{fix_for, Detected};

/// What this repository says, on a machine where codex answers.
fn on_codex() -> Detected {
    Detected {
        adapters: vec![AdapterId::from_static("codex")],
        ..Detected::default()
    }
}

#[test]
fn a_runner_a_node_names_is_declared_on_an_adapter_that_answers() {
    let error = CheckError::UnknownRunner {
        node: NodeId::from("plan"),
        runner: RunnerName::from_static("planner"),
        near: None,
    };
    let fix = fix_for(&error, &on_codex()).expect("a runner to declare");
    assert_eq!(
        fix.paste,
        [
            "runners:",
            "  planner:",
            "    - { adapter: codex, model: <model> }"
        ]
    );
}

#[test]
fn a_runner_one_slip_from_a_declared_one_is_a_typo_with_nothing_to_declare() {
    let error = CheckError::UnknownRunner {
        node: NodeId::from("plan"),
        runner: RunnerName::from_static("planer"),
        near: Some("planner".to_string()),
    };
    assert_eq!(fix_for(&error, &on_codex()), None);
}

#[test]
fn a_node_that_needs_the_default_runner_declares_one_and_the_default() {
    let error = CheckError::Unset {
        node: NodeId::from("plan"),
        key: ConfigKey::Runner,
        near: None,
    };
    let fix = fix_for(&error, &on_codex()).expect("a runner to declare");
    assert!(
        fix.paste
            .ends_with(&["defaults:".to_string(), "  runner: implementer".to_string()]),
        "{fix:?}"
    );
}

#[test]
fn a_command_this_repository_answers_for_is_fixed_by_its_detected_value() {
    let mut detected = on_codex();
    detected
        .commands
        .insert(CommandName::from_static("lint"), "pnpm lint".to_string());
    let error = CheckError::Unset {
        node: NodeId::from("lint"),
        key: ConfigKey::Command {
            command: CommandName::from_static("lint"),
        },
        near: None,
    };
    assert_eq!(
        fix_for(&error, &detected),
        Some(Fix::said(
            "detected here: declare `commands: { lint: \"pnpm lint\" }`"
        ))
    );
}
