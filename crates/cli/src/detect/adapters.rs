//! The adapter CLIs this machine answers for, and the runner a project
//! that declares none would declare with them.
//!
//! A probe reports a version and never a model: a model is the project's
//! choice, so what is proposed here leaves it for the person to name.

use yunta_core::port::ProbeReport;
use yunta_core::{AdapterId, AdapterSettings, RunnerName};

/// What probing one adapter this binary builds found.
pub(crate) struct ProbedAdapter {
    pub(crate) id: AdapterId,
    pub(crate) healthy: bool,
    /// The version a healthy one reported, or why one is not.
    pub(crate) detail: String,
}

/// Probes every adapter this binary builds, in the order the composition
/// root declares them — the id each reports about itself, never one
/// re-spelled here.
pub(crate) async fn probe_known_adapters() -> Vec<ProbedAdapter> {
    let mut probed = Vec::new();
    for adapter in crate::commands::built_adapters(|_| AdapterSettings::default()) {
        let id = adapter.id().clone();
        probed.push(match adapter.probe().await {
            Ok(ProbeReport::Healthy { version }) => ProbedAdapter {
                id,
                healthy: true,
                detail: version.unwrap_or_else(|| "version unknown".to_string()),
            },
            Ok(ProbeReport::Unhealthy { diagnostic }) => ProbedAdapter {
                id,
                healthy: false,
                detail: diagnostic,
            },
            Err(e) => ProbedAdapter {
                id,
                healthy: false,
                detail: e.to_string(),
            },
        });
    }
    probed
}

/// The adapters that answered healthy.
pub(crate) fn healthy(probed: &[ProbedAdapter]) -> Vec<AdapterId> {
    probed
        .iter()
        .filter(|adapter| adapter.healthy)
        .map(|adapter| adapter.id.clone())
        .collect()
}

/// The role a project that names none gets proposed: what the skeletons
/// `yunta new` writes name in their comments.
const PROPOSED_RUNNER: &str = "implementer";

/// The lines that say how to declare the runners `roles` names — the
/// proposed role when it names none — each on the first adapter that
/// answered here, and `defaults.runner` when a node names no runner of
/// its own (`needs_default`). Ready to paste: the one placeholder is the
/// model, which only the project can choose.
pub(crate) fn runner_step(
    roles: &[RunnerName],
    needs_default: bool,
    healthy: &[AdapterId],
) -> Vec<String> {
    let proposed = [RunnerName::from_static(PROPOSED_RUNNER)];
    let roles = match roles.is_empty() {
        true => &proposed[..],
        false => roles,
    };
    let (adapter, where_from) = match healthy.first() {
        Some(first) => (
            first.to_string(),
            format!(
                "this machine answers for {}",
                yunta_core::text::listed(healthy.iter().map(AdapterId::as_str))
            ),
        ),
        None => (
            "<adapter>".to_string(),
            format!(
                "no adapter CLI answered on this machine (this build knows {})",
                crate::commands::built_adapter_names()
            ),
        ),
    };
    let mut lines = vec![format!(
        "declare a runner in .yunta/config.yaml — {where_from}:"
    )];
    lines.push("    runners:".to_string());
    for role in roles {
        lines.push(format!("      {role}:"));
        lines.push(format!(
            "        - {{ adapter: {adapter}, model: <model> }}"
        ));
    }
    if let (true, Some(first)) = (needs_default, roles.first()) {
        lines.push("    defaults:".to_string());
        lines.push(format!("      runner: {first}"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runner_step_names_the_adapters_this_machine_answers_for() {
        let healthy = [
            AdapterId::from_static("claude-code"),
            AdapterId::from_static("codex"),
        ];
        assert_eq!(
            runner_step(&[], true, &healthy),
            vec![
                "declare a runner in .yunta/config.yaml — this machine answers for \
                 `claude-code`, `codex`:",
                "    runners:",
                "      implementer:",
                "        - { adapter: claude-code, model: <model> }",
                "    defaults:",
                "      runner: implementer",
            ]
        );
    }

    #[test]
    fn a_runner_step_declares_each_role_a_workflow_names() {
        let lines = runner_step(
            &[RunnerName::from_static("planner")],
            false,
            &[AdapterId::from_static("codex")],
        );
        assert!(lines.contains(&"      planner:".to_string()), "{lines:?}");
        assert!(
            !lines.iter().any(|line| line.contains("defaults")),
            "a node that names its runner needs no default: {lines:?}"
        );
    }

    #[test]
    fn with_no_adapter_answering_the_step_says_so_and_leaves_it_to_fill() {
        let lines = runner_step(&[], false, &[]);
        assert!(lines[0].contains("no adapter CLI answered"), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("adapter: <adapter>")));
    }
}
