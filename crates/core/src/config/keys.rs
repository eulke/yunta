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
use crate::ids::ExecutorName;
use crate::workflow::{CheckBuiltin, Node, NodeKind};

/// A config key a node's kind cannot run without.
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
}

impl ConfigKey {
    /// The key `config` leaves unset for `node`, if any.
    ///
    /// The baseline is not asked here: a run measures it once for its
    /// whole lineage, so whether a comparison has something to compare
    /// against is a question about the run that started the lineage, not
    /// about the node.
    pub fn unset(node: &Node, config: &ConfigLayer) -> Option<ConfigKey> {
        match &node.kind {
            NodeKind::Check(CheckBuiltin::CoverageGate) if config.coverage.is_none() => {
                Some(ConfigKey::Coverage)
            }
            NodeKind::Executor { executor, .. } if !registered(config, executor) => {
                Some(ConfigKey::Executor {
                    executor: executor.clone(),
                })
            }
            NodeKind::Prompt { .. } | NodeKind::Loop { .. }
                if node.runner.is_none()
                    && node.runners.is_empty()
                    && config
                        .defaults
                        .as_ref()
                        .and_then(|defaults| defaults.runner.as_ref())
                        .is_none() =>
            {
                Some(ConfigKey::Runner)
            }
            _ => None,
        }
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
        }
    }
}
