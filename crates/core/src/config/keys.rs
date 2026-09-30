//! The config keys a node cannot run without.
//!
//! A run freezes its config when it is created, so a key the config
//! leaves unset stops the node every time it runs, whatever a person does
//! in between. That makes it the same fact in two places: `check` refuses
//! the workflow for it before the first token, and a run that meets it
//! anyway fails the node with the same words. Both read it here.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::ConfigLayer;
use crate::ids::{CommandName, ExecutorName};
use crate::workflow::{node_commands, CheckBuiltin, Node, NodeKind};

/// A config key a node cannot run without.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "key", rename_all = "snake_case")]
pub enum ConfigKey {
    /// `baseline_compare` compares against what the lineage measured
    /// before its first node, and `baseline.suite` is what it measures.
    BaselineSuite,
    /// `coverage_gate` runs `coverage.cmd` and holds it to
    /// `coverage.threshold`.
    Coverage,
    /// `kind: executor` runs the binary registered under its name.
    Executor { executor: ExecutorName },
    /// A session opens on a runner: the node's own, or `defaults.runner`.
    Runner,
    /// `run: { command: <name> }` runs what the project declares under
    /// `commands:` for that name.
    Command { command: CommandName },
}

impl ConfigKey {
    /// Every key `node` needs the config to declare, in the order a run
    /// meets them and each once — asked of the node alone, so a pack's
    /// workflow says what it needs before any project reads it.
    ///
    /// The baseline is not asked here: a run measures it once for its
    /// whole lineage, so whether a comparison has something to compare
    /// against is a question about the run that started the lineage, not
    /// about the node.
    pub fn needed_by(node: &Node) -> Vec<ConfigKey> {
        let mut needed = Vec::new();
        match &node.kind {
            NodeKind::Check(CheckBuiltin::CoverageGate) => needed.push(ConfigKey::Coverage),
            NodeKind::Executor { executor, .. } => needed.push(ConfigKey::Executor {
                executor: executor.clone(),
            }),
            NodeKind::Prompt { .. } | NodeKind::Loop { .. }
                if node.runner.is_none() && node.runners.is_empty() =>
            {
                needed.push(ConfigKey::Runner);
            }
            _ => {}
        }
        for command in node_commands(node).filter_map(|run| run.project()) {
            let key = ConfigKey::Command {
                command: command.clone(),
            };
            if !needed.contains(&key) {
                needed.push(key);
            }
        }
        needed
    }

    /// Whether `config` declares this key.
    pub fn is_declared(&self, config: &ConfigLayer) -> bool {
        match self {
            ConfigKey::BaselineSuite => config.baseline.is_some(),
            ConfigKey::Coverage => config.coverage.is_some(),
            ConfigKey::Executor { executor } => registered(config, executor),
            ConfigKey::Runner => config
                .defaults
                .as_ref()
                .is_some_and(|defaults| defaults.runner.is_some()),
            ConfigKey::Command { command } => config.command(command).is_some(),
        }
    }

    /// What a project lacks when it leaves this key unset, as the end of
    /// "the project declares …".
    pub fn undeclared(&self) -> String {
        match self {
            ConfigKey::BaselineSuite => "no `baseline.suite`".to_string(),
            ConfigKey::Coverage => "no `coverage`".to_string(),
            ConfigKey::Executor { executor } => format!("no executor `{executor}`"),
            ConfigKey::Runner => "no `defaults.runner`".to_string(),
            ConfigKey::Command { command } => format!("no command `{command}`"),
        }
    }

    /// Every key `node` needs that `config` leaves unset.
    pub fn unset(node: &Node, config: &ConfigLayer) -> Vec<ConfigKey> {
        ConfigKey::needed_by(node)
            .into_iter()
            .filter(|key| !key.is_declared(config))
            .collect()
    }
}

fn registered(config: &ConfigLayer, executor: &ExecutorName) -> bool {
    config
        .skills
        .as_ref()
        .is_some_and(|skills| skills.executors.iter().any(|e| e.name == *executor))
}

impl fmt::Display for ConfigKey {
    /// What is missing and what to declare, in one sentence that names
    /// no node: the caller already says which node it is about.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigKey::BaselineSuite => f.write_str(
                "`baseline_compare` has nothing to compare against: the config declares no \
                 `baseline.suite`, so nothing is measured before the first node — declare one",
            ),
            ConfigKey::Coverage => f.write_str(
                "`coverage_gate` has nothing to measure: the config declares no `coverage` — \
                 declare `coverage.cmd` and `coverage.threshold`",
            ),
            ConfigKey::Executor { executor } => write!(
                f,
                "executor `{executor}` is not registered — declare it under `skills.executors`"
            ),
            ConfigKey::Runner => f.write_str(
                "it opens a session and names no runner, and the config declares no \
                 `defaults.runner` — name a `runner:` on the node, or declare `defaults.runner`",
            ),
            ConfigKey::Command { command } => write!(
                f,
                "it runs the project's command `{command}`, and the config declares none — \
                 declare what this project runs for it under `commands.{command}`"
            ),
        }
    }
}
