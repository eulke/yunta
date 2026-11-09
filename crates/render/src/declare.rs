//! What to declare in a project's config, as a person pastes it: a key a
//! workflow needs, as the repository was detected to answer it, and the
//! runners a workflow names, on the adapters the machine answers for.
//!
//! A probe reports a version and never a model: a model is the project's
//! choice, so the runner proposed here leaves it for the person to name.

use surco::components::Fix;
use yunta_core::{AdapterId, ConfigKey, Detected, MachineAdapters, RunnerName};

/// What to declare for `key`, as `detected` answers it — `None` when it
/// answers nothing for it.
pub fn as_detected(detected: &Detected, key: &ConfigKey) -> Option<String> {
    match key {
        ConfigKey::Command { command } => detected
            .commands
            .get(command)
            .map(|text| format!("detected here: declare `commands: {{ {command}: \"{text}\" }}`")),
        ConfigKey::BaselineSuite => detected
            .suite
            .as_ref()
            .map(|suite| format!("detected here: declare `baseline: {{ suite: \"{suite}\" }}`")),
        ConfigKey::Forge => detected.forge.as_ref().map(|repo| {
            format!(
                "detected here: declare `forge: {{ github: {{ repo: {repo}, token_env: \
                 GITHUB_TOKEN }} }}`"
            )
        }),
        ConfigKey::Coverage
        | ConfigKey::Executor { .. }
        | ConfigKey::Runner
        | ConfigKey::RunBranch => None,
    }
}

/// The role a project that names none gets proposed: what the skeletons
/// `yunta new` writes name in their comments.
const PROPOSED_RUNNER: &str = "implementer";

/// The lines that say how to declare the runners `roles` names — the
/// proposed role when it names none — each on the first adapter that
/// answered on the machine, and `defaults.runner` when a node names no
/// runner of its own (`needs_default`). Ready to paste: the one placeholder
/// is the model, which only the project can choose.
pub fn runners(roles: &[RunnerName], needs_default: bool, adapters: &MachineAdapters) -> Fix {
    let proposed = [RunnerName::from_static(PROPOSED_RUNNER)];
    let roles = match roles.is_empty() {
        true => &proposed[..],
        false => roles,
    };
    let (adapter, where_from) = match adapters.healthy.first() {
        Some(first) => (
            first.to_string(),
            format!(
                "this machine answers for {}",
                surco::text::listed(adapters.healthy.iter().map(AdapterId::as_str))
            ),
        ),
        None => (
            "<adapter>".to_string(),
            format!(
                "no adapter CLI answered on this machine (this build knows {})",
                surco::text::listed(adapters.known.iter().map(AdapterId::as_str))
            ),
        ),
    };
    let mut paste = vec!["runners:".to_string()];
    for role in roles {
        paste.push(format!("  {role}:"));
        paste.push(format!("    - {{ adapter: {adapter}, model: <model> }}"));
    }
    if let (true, Some(first)) = (needs_default, roles.first()) {
        paste.push("defaults:".to_string());
        paste.push(format!("  runner: {first}"));
    }
    Fix {
        said: format!("declare a runner in .yunta/config.yaml; {where_from}:"),
        paste,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runner_to_declare_names_the_adapters_the_machine_answers_for() {
        let adapters = MachineAdapters {
            healthy: vec![
                AdapterId::from_static("claude-code"),
                AdapterId::from_static("codex"),
            ],
            ..MachineAdapters::default()
        };
        let step = runners(&[], true, &adapters);
        assert_eq!(
            step.said,
            "declare a runner in .yunta/config.yaml; this machine answers for `claude-code`, \
             `codex`:"
        );
        assert_eq!(
            step.paste,
            vec![
                "runners:",
                "  implementer:",
                "    - { adapter: claude-code, model: <model> }",
                "defaults:",
                "  runner: implementer",
            ]
        );
    }

    #[test]
    fn a_runner_to_declare_declares_each_role_a_workflow_names() {
        let adapters = MachineAdapters {
            healthy: vec![AdapterId::from_static("codex")],
            ..MachineAdapters::default()
        };
        let lines = runners(&[RunnerName::from_static("planner")], false, &adapters).paste;
        assert!(lines.contains(&"  planner:".to_string()), "{lines:?}");
        assert!(
            !lines.iter().any(|line| line.contains("defaults")),
            "a node that names its runner needs no default: {lines:?}"
        );
    }

    #[test]
    fn with_no_adapter_answering_it_says_so_names_the_known_ones_and_leaves_it_to_fill() {
        let adapters = MachineAdapters {
            healthy: Vec::new(),
            known: vec![AdapterId::from_static("claude-code")],
        };
        let step = runners(&[], false, &adapters);
        assert_eq!(
            step.said,
            "declare a runner in .yunta/config.yaml; no adapter CLI answered on this machine \
             (this build knows `claude-code`):"
        );
        assert!(step
            .paste
            .iter()
            .any(|line| line.contains("adapter: <adapter>")));
    }
}
