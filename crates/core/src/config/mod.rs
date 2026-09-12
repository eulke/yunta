//! Layered config types: every group of the
//! reference config parses and round-trips — `runners`, `adapters`,
//! `mcp_servers`, `skills`, `baseline`/`coverage`, `storage`, `limits`,
//! `paths`, `defaults`, `permissions`, `pricing`, `forge`, `secrets` and
//! `version`. Each field entered with its consumer or an explicit refusal
//! in `check` — never accepted and silently ignored.
//!
//! Merge semantics: maps merge key by key, more specific layer
//! wins per key; arrays (like a role's candidate list) replace wholesale
//! rather than concatenate. Precedence is repo > usuario > org — with one
//! deliberate exception: `permissions` inverts it, the org
//! layer is a ceiling and lower layers only narrow (see
//! [`PermissionsConfig`] and [`permission_layer_conflicts`]).

mod env;
mod merge;
mod permissions;
mod sections;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{AdapterId, RunnerName};
use crate::workflow::OnInterrupt;
use env::expand_path;
use merge::merge;

pub use env::{user_state_root, Env, HomeExpansionError};
pub use permissions::{
    permission_layer_conflicts, CommandPermissions, NetworkPermissions, PackExecutorPolicy,
    PackPermissions, PermissionsConfig, PublisherPermissions,
};
pub use sections::{
    AdapterSettings, BaselineConfig, CoverageConfig, DefaultOnFailure, DefaultsConfig,
    ExecutorKind, ExecutorRegistration, ForgeConfig, GitHubForgeConfig, Isolation, LimitsConfig,
    McpServerConfig, PathsConfig, PricingEntry, ProjectConfig, RunnerCandidate, SkillsConfig,
    StorageConfig,
};

/// One config layer as parsed from a single file (project/user/org), and
/// also the type of the merged result — merging never needs to invent
/// fields, only combine what layers actually set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigLayer {
    /// `version: 1` (reference config) — the layer file's own format
    /// version. The loader refuses any value this binary doesn't speak.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runners: Option<BTreeMap<RunnerName, Vec<RunnerCandidate>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapters: Option<BTreeMap<AdapterId, AdapterSettings>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<BTreeMap<String, McpServerConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<PathsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defaults: Option<DefaultsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<BaselineConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<LimitsConfig>,
    /// `pricing:` — `{model: cost_per_1k_tokens}`, an
    /// optional currency conversion `yunta stats` and the receipt add
    /// *alongside* their token figures, never in place of them. Absent
    /// means everything stays in tokens — the engine has no opinion of
    /// its own on what a token costs, and never invents one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<BTreeMap<String, PricingEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge: Option<ForgeConfig>,
    /// `secrets:` — env var *names* a session may receive; values
    /// only ever come from the process environment at spawn time, and
    /// nothing undeclared reaches a session's env at all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
}

impl ConfigLayer {
    /// Expands a leading `~` in every path this layer declares —
    /// `adapters.<id>.binary`, `storage.path`, `paths.runs`,
    /// `paths.worktrees`, `skills.paths[]` — against `home`, so no
    /// consumer ever sees a literal `~`. `home: None` makes any such path
    /// an error naming the field.
    pub fn expand_home(
        &mut self,
        home: Option<&std::path::Path>,
    ) -> Result<(), HomeExpansionError> {
        if let Some(adapters) = &mut self.adapters {
            for (id, settings) in adapters.iter_mut() {
                if let Some(binary) = &mut settings.binary {
                    expand_path(binary, home, &format!("adapters.{id}.binary"))?;
                }
            }
        }
        if let Some(path) = self
            .storage
            .as_mut()
            .and_then(|storage| storage.path.as_mut())
        {
            expand_path(path, home, "storage.path")?;
        }
        if let Some(paths) = &mut self.paths {
            if let Some(runs) = &mut paths.runs {
                expand_path(runs, home, "paths.runs")?;
            }
            if let Some(worktrees) = &mut paths.worktrees {
                expand_path(worktrees, home, "paths.worktrees")?;
            }
        }
        if let Some(skills) = &mut self.skills {
            for (index, path) in skills.paths.iter_mut().enumerate() {
                expand_path(path, home, &format!("skills.paths[{index}]"))?;
            }
        }
        Ok(())
    }

    /// Merges layers in increasing order of precedence — pass
    /// `[org, user, repo]` so the last one's keys win (the default
    /// precedence; `permissions` inverts this).
    pub fn merge_layers(layers: impl IntoIterator<Item = ConfigLayer>) -> ConfigLayer {
        layers.into_iter().fold(ConfigLayer::default(), merge)
    }

    /// `defaults.isolation`, with the schema's own default (`worktree`)
    /// applied — the one place that default lives, so nothing downstream
    /// re-invents "absent means what?".
    pub fn resolved_isolation(&self) -> Isolation {
        self.defaults
            .as_ref()
            .and_then(|defaults| defaults.isolation)
            .unwrap_or_default()
    }

    /// `defaults.max_parallel_nodes`, with the schema's own default (`1`,
    /// sequential — same place the default lives, same reasoning as
    /// `resolved_isolation`).
    pub fn resolved_max_parallel_nodes(&self) -> u32 {
        self.defaults
            .as_ref()
            .and_then(|defaults| defaults.max_parallel_nodes)
            .unwrap_or(1)
    }

    /// `defaults.on_interrupt`, with the schema's own default
    /// (`restart_node`) applied — a node's own `on_interrupt` still wins
    /// over this when it declares one.
    pub fn resolved_on_interrupt(&self) -> OnInterrupt {
        self.defaults
            .as_ref()
            .and_then(|defaults| defaults.on_interrupt)
            .unwrap_or_default()
    }

    /// `defaults.on_failure`, with the schema's own default (`pause`)
    /// applied — what a failed node with no `on_failure:` re-route of its
    /// own does to the run: `pause` freezes it resumable, `abort` closes
    /// it failed at once, `continue` skips the failed node's dependents
    /// and closes failed once the rest of the graph has run.
    pub fn resolved_on_failure(&self) -> DefaultOnFailure {
        self.defaults
            .as_ref()
            .and_then(|defaults| defaults.on_failure)
            .unwrap_or(DefaultOnFailure::Pause)
    }

    /// `limits.max_artifact_repairs`, with the reference default (`2`)
    /// applied.
    ///
    /// One was the earlier number, on the reasoning that a rewrite
    /// holding the diagnostics either converges on the first attempt or
    /// does not converge. Measured against real sessions, that is not
    /// how it goes: refused ledgers rewritten with their diagnostics in
    /// hand converged five times in six on the first attempt and six in
    /// six on the second. A document can also owe two rounds by
    /// construction — a value of the wrong type stops the parse, so the
    /// rules that only hold across a parsed document cannot be reported
    /// in the same breath, and the writer meets them one round later.
    pub fn resolved_max_artifact_repairs(&self) -> u32 {
        self.limits
            .as_ref()
            .and_then(|limits| limits.max_artifact_repairs)
            .unwrap_or(2)
    }

    /// `defaults.timeout_minutes` as a session `Budget.timeout` — no
    /// hidden default: absent means unlimited, exactly as before.
    pub fn resolved_session_timeout(&self) -> Option<std::time::Duration> {
        self.defaults
            .as_ref()
            .and_then(|defaults| defaults.timeout_minutes)
            .map(|minutes| std::time::Duration::from_secs(minutes * 60))
    }

    /// `limits.max_loop_iterations`, with the reference default (`12`)
    /// applied — the only net under a ledger whose state oscillates
    /// forever, so "absent" means the reference cap, never "unbounded".
    /// `limits.max_workflow_depth`, with the reference default (`4`)
    /// applied — how many `kind: workflow` nesting levels below the
    /// root run are allowed (a configurable maximum depth, checked
    /// statically at graph-build time and again at runtime when a
    /// child run is created).
    pub fn resolved_max_workflow_depth(&self) -> u32 {
        self.limits
            .as_ref()
            .and_then(|limits| limits.max_workflow_depth)
            .unwrap_or(4)
    }

    pub fn resolved_max_loop_iterations(&self) -> u32 {
        self.limits
            .as_ref()
            .and_then(|limits| limits.max_loop_iterations)
            .unwrap_or(12)
    }

    /// `limits.max_expansion_files`, with the reference default (`5`)
    /// applied — the ceiling above which a `rules`-mode scope expansion
    /// is no longer "a small, adjacent set" and is denied. Same "the
    /// default lives here" convention as [`ConfigLayer::resolved_isolation`].
    pub fn resolved_max_expansion_files(&self) -> usize {
        self.limits
            .as_ref()
            .and_then(|limits| limits.max_expansion_files)
            .unwrap_or(5)
    }

    /// `limits.inline_context_bytes`, with the reference default
    /// (`32000`) applied — same "the default lives here" convention as
    /// [`ConfigLayer::resolved_isolation`].
    pub fn resolved_inline_context_bytes(&self) -> u64 {
        self.limits
            .as_ref()
            .and_then(|limits| limits.inline_context_bytes)
            .unwrap_or(32_000)
    }
}
