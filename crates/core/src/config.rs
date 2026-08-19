//! Layered config types (T1.2) — **M-0 cut only**.
//!
//! Full T1.2 covers eight groups (`runners`, `adapters`, `mcp_servers`,
//! `skills`, `baseline`/`coverage`, `storage`, `limits`, `paths`,
//! `secrets`, `permissions` with its inverted merge, D51/§6.1). M-0 is not
//! named in the Plan's own M-0 scope section at all, so this only builds
//! the four groups something already planned for M-0 actually consumes:
//! `runners` (resolves `Node.runner`, T1.1), `adapters` (T3.1/T7.3
//! settings), `storage` (T2.1's SQLite path) and `paths` (run.dir/worktree
//! locations for T7.1's real `resume`). The rest extends this module when
//! its own consumer lands — not before (CLAUDE.md: "scope chico y
//! declarado").
//!
//! Merge semantics (§2.2, D52): maps merge key by key, more specific layer
//! wins per key; arrays (like a role's candidate list) replace wholesale
//! rather than concatenate. Precedence for these four groups is
//! repo > usuario > org — `permissions`' inverted precedence (org wins)
//! is out of scope until `permissions` itself is implemented.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::workflow::OnInterrupt;

/// One binding candidate for a role in `runners:` (Contrato §13.1, I17).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunnerCandidate {
    pub adapter: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// Adapter-specific settings that have a portable expression (D29): for
/// now just a binary path override, matching the reference config.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AdapterSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary: Option<PathBuf>,
}

/// `storage:` (D53 — SQLite is the only backend).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
}

/// `paths:` (§2.2, D52) — where run/worktree state lives. `YUNTA_HOME` is
/// an environment override applied when resolving the merged config, not
/// a field of it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PathsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktrees: Option<PathBuf>,
}

/// How a first-level run isolates its working tree from the checkout
/// that started it (§7.3, T4.2). `worktree` (default) gives each run its
/// own `git worktree`; `none` operates directly on the given checkout,
/// legitimate for watching an agent edit live or for CI already inside
/// an ephemeral container. `inherit` (§12, sub-runs only) isn't a value
/// here — a first-level run has no parent to inherit from — and
/// `container` isn't a schema value at all (A-09, undesigned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    #[default]
    Worktree,
    None,
}

/// `defaults:` — **M-0 cut**: `isolation` (T4.2's consumer) and
/// `max_parallel_nodes` (T4.1's consumer). The reference config's
/// `runner`/`timeout_minutes`/`on_failure`/`on_interrupt` wait for their
/// own consumers (T4.4/T4.5) — same "extend when consumed" rule as every
/// other group here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DefaultsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<Isolation>,
    /// How many DAG nodes with no dependency on each other the scheduler
    /// may run at once (T4.1). Absent means the schema's own default of
    /// `1`, not "unbounded" — §5.5 states the analogous rationale for
    /// `concurrency` in loops and it applies just as much here: nobody
    /// should discover parallel token spend by reading the bill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallel_nodes: Option<u32>,
    /// Fallback `on_interrupt` (§8.1, T4.5) a node without its own
    /// override resolves to. Same default (`restart_node`) as the
    /// schema's own, so absent here changes nothing either.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_interrupt: Option<OnInterrupt>,
}

/// One config layer as parsed from a single file (project/user/org), and
/// also the type of the merged result — merging never needs to invent
/// fields, only combine what layers actually set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigLayer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runners: Option<HashMap<String, Vec<RunnerCandidate>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapters: Option<HashMap<String, AdapterSettings>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<PathsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defaults: Option<DefaultsConfig>,
}

impl ConfigLayer {
    /// Merges layers in increasing order of precedence — pass
    /// `[org, user, repo]` so the last one's keys win (D52/§2.2's default
    /// precedence; `permissions` will invert this once it exists).
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
    /// over this when it declares one (T4.5's per-node override).
    pub fn resolved_on_interrupt(&self) -> OnInterrupt {
        self.defaults
            .as_ref()
            .and_then(|defaults| defaults.on_interrupt)
            .unwrap_or_default()
    }
}

fn merge(base: ConfigLayer, more_specific: ConfigLayer) -> ConfigLayer {
    ConfigLayer {
        runners: merge_map_replacing_values(base.runners, more_specific.runners),
        adapters: merge_map_of_fields(
            base.adapters,
            more_specific.adapters,
            merge_adapter_settings,
        ),
        storage: merge_fields(base.storage, more_specific.storage, merge_storage_config),
        paths: merge_fields(base.paths, more_specific.paths, merge_paths_config),
        defaults: merge_fields(base.defaults, more_specific.defaults, merge_defaults_config),
    }
}

fn merge_defaults_config(base: DefaultsConfig, more_specific: DefaultsConfig) -> DefaultsConfig {
    DefaultsConfig {
        isolation: more_specific.isolation.or(base.isolation),
        max_parallel_nodes: more_specific.max_parallel_nodes.or(base.max_parallel_nodes),
        on_interrupt: more_specific.on_interrupt.or(base.on_interrupt),
    }
}

/// A map whose values are arrays: per §2.2 "arrays reemplazan", a key
/// present in the more specific layer replaces the base's value for that
/// key wholesale, rather than concatenating the two arrays.
fn merge_map_replacing_values<K, V>(
    base: Option<HashMap<K, Vec<V>>>,
    more_specific: Option<HashMap<K, Vec<V>>>,
) -> Option<HashMap<K, Vec<V>>>
where
    K: Eq + std::hash::Hash,
{
    merge_maps(base, more_specific, |_base_value, override_value| {
        override_value
    })
}

/// A map whose values are themselves mergeable structs (field by field).
fn merge_map_of_fields<K, V>(
    base: Option<HashMap<K, V>>,
    more_specific: Option<HashMap<K, V>>,
    merge_value: impl Fn(V, V) -> V,
) -> Option<HashMap<K, V>>
where
    K: Eq + std::hash::Hash,
{
    merge_maps(base, more_specific, merge_value)
}

fn merge_maps<K, V>(
    base: Option<HashMap<K, V>>,
    more_specific: Option<HashMap<K, V>>,
    merge_value: impl Fn(V, V) -> V,
) -> Option<HashMap<K, V>>
where
    K: Eq + std::hash::Hash,
{
    match (base, more_specific) {
        (None, None) => None,
        (Some(base), None) => Some(base),
        (None, Some(more_specific)) => Some(more_specific),
        (Some(mut base), Some(more_specific)) => {
            for (key, value) in more_specific {
                let merged = match base.remove(&key) {
                    Some(base_value) => merge_value(base_value, value),
                    None => value,
                };
                base.insert(key, merged);
            }
            Some(base)
        }
    }
}

fn merge_fields<T>(
    base: Option<T>,
    more_specific: Option<T>,
    merge_value: impl Fn(T, T) -> T,
) -> Option<T> {
    match (base, more_specific) {
        (None, None) => None,
        (Some(base), None) => Some(base),
        (None, Some(more_specific)) => Some(more_specific),
        (Some(base), Some(more_specific)) => Some(merge_value(base, more_specific)),
    }
}

fn merge_adapter_settings(
    base: AdapterSettings,
    more_specific: AdapterSettings,
) -> AdapterSettings {
    AdapterSettings {
        binary: more_specific.binary.or(base.binary),
    }
}

fn merge_storage_config(base: StorageConfig, more_specific: StorageConfig) -> StorageConfig {
    StorageConfig {
        path: more_specific.path.or(base.path),
        retention_days: more_specific.retention_days.or(base.retention_days),
    }
}

fn merge_paths_config(base: PathsConfig, more_specific: PathsConfig) -> PathsConfig {
    PathsConfig {
        runs: more_specific.runs.or(base.runs),
        worktrees: more_specific.worktrees.or(base.worktrees),
    }
}
