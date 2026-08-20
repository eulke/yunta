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
//! rather than concatenate. Precedence is repo > usuario > org — with one
//! deliberate exception: `permissions` (§6.1, T5.7) inverts it, the org
//! layer is a ceiling and lower layers only narrow (see
//! [`PermissionsConfig`] and [`permission_layer_conflicts`]).

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

/// One server in `mcp_servers:` (§9.2, T6.2) — the reference config's own
/// shape: a streamable-HTTP endpoint plus the *name* of an env var
/// carrying the bearer token, never the token itself (I12/O3: secrets are
/// env var names in config, values only ever come from the process
/// environment at resolve time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_env: Option<String>,
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

/// `project:` (§9's own `{{project.*}}` template namespace, T6.3) — the
/// reference config's own three fields. M-0 cut: read-only data for
/// templates, nothing here drives behavior yet (`base_branch` isn't
/// consulted by any re-route/PR logic in this recorte).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_prefix: Option<String>,
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

/// The user state root: `$YUNTA_HOME`, or `~/.yunta` when unset. Shared by
/// the CLI (which layers `config.yaml` from it, T1.2) and the engine
/// (which reads `knowledge/` from it live at context-resolution time,
/// T6.5) so the two never drift on what "the user layer" means. `None`
/// only when neither `YUNTA_HOME` nor `HOME` is set.
pub fn user_state_root() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("YUNTA_HOME") {
        return Some(PathBuf::from(home));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".yunta"))
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

/// `baseline:` (§7.2, T5.4's `baseline_compare`) — the suite the engine
/// runs and re-runs to catch regressions ("cero regresiones" as a data
/// comparison, never an agent's claim).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineConfig {
    pub suite: String,
}

/// `coverage:` (§7.2, T5.4's `coverage_gate`) — `cmd`'s stdout must
/// contain a bare percentage (`NN[.NN]%`); the last match found is taken
/// as the measured coverage. Not specified by the Contrato's prose,
/// which only says "medido y comparado por el engine" — a permissive,
/// documented convention rather than inventing a stricter parsing
/// contract with no source to check it against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageConfig {
    pub cmd: String,
    pub threshold: f64,
}

/// `skills:` (D47/D87, T5.6) — M-0 cut carries only `executors`, the one
/// sub-field `kind: executor` needs to resolve its own `executor:` name
/// to a binary on disk. `paths`/`always` (skill discovery and injection
/// into a node's assembled context) are M6's context-assembly work, with
/// no consumer yet in this recorte.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillsConfig {
    #[serde(default)]
    pub executors: Vec<ExecutorRegistration>,
}

/// One `skills.executors:` entry — `name` is what a `kind: executor`
/// node's own `executor:` field references.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutorRegistration {
    pub name: String,
    pub kind: ExecutorKind,
    pub path: PathBuf,
}

/// Closed at `binary` today — D47 explicitly reserves `wasm` as a future
/// additive variant ("`kind: wasm` queda como extensión aditiva futura si
/// los datos la piden"), so this is an enum even with a single variant,
/// not a bare string that would silently accept anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorKind {
    Binary,
}

/// `permissions:` (§6.1, D51, I18, T5.7) — ONE model of ceilings, not
/// loose mechanisms: each level may only narrow the one above, never
/// loosen it. Unlike every other config group (repo > user > org), the
/// org layer rules here and lower layers only restrict further — without
/// that inversion, governance is theater: any repo could undo it.
///
/// This is governance, not a sandbox (§6.1's own honest limit): an agent
/// with write access can route around a textual pattern by writing a
/// script and running it. The model stops the accident and the careless
/// pack, and leaves an auditable trail of the deliberate attempt — real
/// isolation belongs to the execution environment, never to Yunta.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PermissionsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<CommandPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packs: Option<PackPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkPermissions>,
}

/// `permissions.commands` — patterns matched against every hook, criterion,
/// bash node and executor command right before it runs (§6.1). Empty
/// `allow` = denylist mode (everything not denied runs); a non-empty
/// `allow` is D51's "allowlist opcional estricta".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CommandPermissions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
}

/// `permissions.packs` — governance over pack contents (D51/D72). Parsed
/// and merged here from T5.7; *enforced* at `pack add`/check when packs
/// themselves land (M11, T11.5) — a key without its consumer yet, kept
/// because the org ceiling file is one document and its schema shouldn't
/// dribble in per-milestone.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackPermissions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executors: Option<PackExecutorPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publishers: Option<PublisherPermissions>,
}

/// `allow | prompt | deny`, strictly ordered: `Deny` is the narrowest,
/// `Allow` the loosest — the ceiling merge keeps the strictest across
/// layers. `prompt` asks for confirmation at `yunta pack add` (D72),
/// never mid-run: runs are headless, humans interact through gates only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackExecutorPolicy {
    Allow,
    Prompt,
    Deny,
}

impl PackExecutorPolicy {
    fn strictness(self) -> u8 {
        match self {
            PackExecutorPolicy::Allow => 0,
            PackExecutorPolicy::Prompt => 1,
            PackExecutorPolicy::Deny => 2,
        }
    }
}

/// `permissions.packs.publishers` — `allow` empty means every publisher
/// is accepted (§6.1's "vacío = todos").
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PublisherPermissions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
}

/// `permissions.network` — declarative ONLY (D105): `default: false`
/// activates no sandboxing whatsoever. It exists for policy and audit; an
/// executor that wants to actually enforce it does so on its own. Policy
/// ≠ capability ≠ OS enforcement — Yunta core never promises the third.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPermissions {
    pub default: bool,
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
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,
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
    /// `pricing:` (§8.4, T7.5) — `{model: cost_per_1k_tokens}`, an
    /// optional currency conversion `yunta stats` and the receipt add
    /// *alongside* their token figures, never in place of them. Absent
    /// means everything stays in tokens — the engine has no opinion of
    /// its own on what a token costs, and never invents one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<HashMap<String, f64>>,
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
        mcp_servers: merge_maps(
            base.mcp_servers,
            more_specific.mcp_servers,
            |_base, more| more,
        ),
        // Per-model, same as `mcp_servers`: a repo layer overriding one
        // model's price doesn't discard the rest a user/org layer priced.
        pricing: merge_maps(base.pricing, more_specific.pricing, |_base, more| more),
        storage: merge_fields(base.storage, more_specific.storage, merge_storage_config),
        project: merge_fields(base.project, more_specific.project, merge_project_config),
        paths: merge_fields(base.paths, more_specific.paths, merge_paths_config),
        defaults: merge_fields(base.defaults, more_specific.defaults, merge_defaults_config),
        // Every field in these two is required (no internal optionality
        // to merge field-by-field) — a more specific layer replaces the
        // whole group wholesale, same as `runners`' candidate arrays.
        baseline: more_specific.baseline.or(base.baseline),
        coverage: more_specific.coverage.or(base.coverage),
        skills: more_specific.skills.or(base.skills),
        // The deliberate inversion (§6.1): `merge_layers` folds org
        // first, so `base` here is always the HIGHER layer for
        // permissions — the ceiling. The lower layer only ever narrows
        // the result; a loosening attempt is surfaced as an error by
        // [`permission_layer_conflicts`], never silently merged away.
        permissions: merge_permissions(base.permissions, more_specific.permissions),
    }
}

/// Ceiling merge for `permissions` (§6.1/I18): the effective model is the
/// most restrictive combination of both layers, computed conservatively —
/// even when a lower layer *tried* to loosen (a check error via
/// [`permission_layer_conflicts`]), the runtime model never runs anything
/// the ceiling denied.
fn merge_permissions(
    ceiling: Option<PermissionsConfig>,
    lower: Option<PermissionsConfig>,
) -> Option<PermissionsConfig> {
    let (ceiling, lower) = match (ceiling, lower) {
        (None, None) => return None,
        (Some(c), None) => return Some(c),
        (None, Some(l)) => return Some(l),
        (Some(c), Some(l)) => (c, l),
    };

    let commands = match (ceiling.commands, lower.commands) {
        (None, None) => None,
        (Some(c), None) => Some(c),
        (None, Some(l)) => Some(l),
        (Some(c), Some(l)) => {
            // Denies union: adding denies is narrowing, always legal.
            let mut deny = c.deny.clone();
            for pattern in l.deny {
                if !deny.contains(&pattern) {
                    deny.push(pattern);
                }
            }
            // Allows: the ceiling's non-empty allow bounds the lower
            // layer's — entries outside it are dropped here (and reported
            // as conflicts by the checker, not swallowed silently).
            let allow = if c.allow.is_empty() {
                l.allow
            } else if l.allow.is_empty() {
                c.allow
            } else {
                l.allow
                    .into_iter()
                    .filter(|pattern| c.allow.contains(pattern))
                    .collect()
            };
            Some(CommandPermissions { deny, allow })
        }
    };

    let packs = match (ceiling.packs, lower.packs) {
        (None, None) => None,
        (Some(c), None) => Some(c),
        (None, Some(l)) => Some(l),
        (Some(c), Some(l)) => {
            let executors = match (c.executors, l.executors) {
                (Some(a), Some(b)) => Some(if a.strictness() >= b.strictness() {
                    a
                } else {
                    b
                }),
                (a, b) => a.or(b),
            };
            let publishers = match (c.publishers, l.publishers) {
                (None, None) => None,
                (Some(p), None) => Some(p),
                (None, Some(p)) => Some(p),
                (Some(c_pub), Some(l_pub)) => {
                    // Empty = everyone (§6.1) — a non-empty ceiling bounds
                    // the lower list; both non-empty intersect.
                    let allow = if c_pub.allow.is_empty() {
                        l_pub.allow
                    } else if l_pub.allow.is_empty() {
                        c_pub.allow
                    } else {
                        l_pub
                            .allow
                            .into_iter()
                            .filter(|publisher| c_pub.allow.contains(publisher))
                            .collect()
                    };
                    Some(PublisherPermissions { allow })
                }
            };
            Some(PackPermissions {
                executors,
                publishers,
            })
        }
    };

    let network = match (ceiling.network, lower.network) {
        (Some(c), Some(l)) => Some(NetworkPermissions {
            // `false` is the narrower value — a ceiling that turned the
            // default off stays off no matter what a lower layer says.
            default: c.default && l.default,
        }),
        (c, l) => c.or(l),
    };

    Some(PermissionsConfig {
        commands,
        packs,
        network,
    })
}

/// Detects loosening attempts across ordered permission layers (§6.1,
/// T5.7's own acceptance case: "repo que intenta re-permitir un patrón
/// denegado por org: check lo rechaza citando la capa"). `layers` come
/// ordered highest ceiling first (org, then user, then repo); every
/// returned string names the offending layer, the ceiling layer it
/// violated, and the exact pattern — comparison is textual on purpose:
/// mechanical and predictable, no cleverness about glob overlap.
pub fn permission_layer_conflicts(layers: &[(&str, &ConfigLayer)]) -> Vec<String> {
    let mut conflicts = Vec::new();

    for (lower_idx, (lower_name, lower_layer)) in layers.iter().enumerate() {
        let Some(lower) = &lower_layer.permissions else {
            continue;
        };
        for (higher_name, higher_layer) in layers.iter().take(lower_idx) {
            let Some(higher) = &higher_layer.permissions else {
                continue;
            };

            if let (Some(lower_cmds), Some(higher_cmds)) = (&lower.commands, &higher.commands) {
                for pattern in &lower_cmds.allow {
                    if higher_cmds.deny.contains(pattern) {
                        conflicts.push(format!(
                            "layer `{lower_name}` re-allows command pattern `{pattern}` denied by layer `{higher_name}` — permissions only narrow (§6.1)"
                        ));
                    } else if !higher_cmds.allow.is_empty() && !higher_cmds.allow.contains(pattern)
                    {
                        conflicts.push(format!(
                            "layer `{lower_name}` allows command pattern `{pattern}` outside layer `{higher_name}`'s allowlist — permissions only narrow (§6.1)"
                        ));
                    }
                }
            }

            if let (Some(lower_packs), Some(higher_packs)) = (&lower.packs, &higher.packs) {
                if let (Some(lower_pol), Some(higher_pol)) =
                    (lower_packs.executors, higher_packs.executors)
                {
                    if lower_pol.strictness() < higher_pol.strictness() {
                        conflicts.push(format!(
                            "layer `{lower_name}` loosens `packs.executors` to `{lower_pol:?}` below layer `{higher_name}`'s `{higher_pol:?}` — permissions only narrow (§6.1)"
                        ));
                    }
                }
                if let (Some(lower_pub), Some(higher_pub)) =
                    (&lower_packs.publishers, &higher_packs.publishers)
                {
                    if !higher_pub.allow.is_empty() {
                        for publisher in &lower_pub.allow {
                            if !higher_pub.allow.contains(publisher) {
                                conflicts.push(format!(
                                    "layer `{lower_name}` allows publisher `{publisher}` outside layer `{higher_name}`'s allowlist — permissions only narrow (§6.1)"
                                ));
                            }
                        }
                    }
                }
            }

            if let (Some(lower_net), Some(higher_net)) = (lower.network, higher.network) {
                if lower_net.default && !higher_net.default {
                    conflicts.push(format!(
                        "layer `{lower_name}` re-enables `network.default` turned off by layer `{higher_name}` — permissions only narrow (§6.1)"
                    ));
                }
            }
        }
    }

    conflicts
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

fn merge_project_config(base: ProjectConfig, more_specific: ProjectConfig) -> ProjectConfig {
    ProjectConfig {
        name: more_specific.name.or(base.name),
        base_branch: more_specific.base_branch.or(base.base_branch),
        branch_prefix: more_specific.branch_prefix.or(base.branch_prefix),
    }
}

fn merge_paths_config(base: PathsConfig, more_specific: PathsConfig) -> PathsConfig {
    PathsConfig {
        runs: more_specific.runs.or(base.runs),
        worktrees: more_specific.worktrees.or(base.worktrees),
    }
}
