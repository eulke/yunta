//! Layer merge: maps key by key with the more specific layer winning,
//! arrays replaced wholesale, sections field by field.

use std::collections::BTreeMap;

use super::permissions::merge_permissions;
use super::{
    AdapterSettings, ConfigLayer, DefaultsConfig, LimitsConfig, PathsConfig, ProjectConfig,
    StorageConfig,
};

pub(super) fn merge(base: ConfigLayer, more_specific: ConfigLayer) -> ConfigLayer {
    ConfigLayer {
        version: more_specific.version.or(base.version),
        // Union, order-preserving: every layer's declared secret names
        // stand — a repo can add names, never silently drop the org's.
        secrets: {
            let mut secrets = base.secrets;
            for name in more_specific.secrets {
                if !secrets.contains(&name) {
                    secrets.push(name);
                }
            }
            secrets
        },
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
        // Whole-group replace, same as `baseline`/`coverage`: one forge
        // per repo in practice, nothing internal to merge field-by-field.
        forge: more_specific.forge.or(base.forge),
        storage: merge_fields(base.storage, more_specific.storage, merge_storage_config),
        project: merge_fields(base.project, more_specific.project, merge_project_config),
        paths: merge_fields(base.paths, more_specific.paths, merge_paths_config),
        defaults: merge_fields(base.defaults, more_specific.defaults, merge_defaults_config),
        // Budgets, not permissions: normal precedence (repo > user >
        // org), field by field — the inverted ceiling merge below is
        // exclusive to `permissions`.
        limits: merge_fields(base.limits, more_specific.limits, merge_limits_config),
        // Every field in these two is required (no internal optionality
        // to merge field-by-field) — a more specific layer replaces the
        // whole group wholesale, same as `runners`' candidate arrays.
        baseline: more_specific.baseline.or(base.baseline),
        coverage: more_specific.coverage.or(base.coverage),
        skills: more_specific.skills.or(base.skills),
        // The deliberate inversion: `merge_layers` folds org
        // first, so `base` here is always the HIGHER layer for
        // permissions — the ceiling. The lower layer only ever narrows
        // the result; a loosening attempt is surfaced as an error by
        // [`permission_layer_conflicts`], never silently merged away.
        permissions: merge_permissions(base.permissions, more_specific.permissions),
    }
}

fn merge_defaults_config(base: DefaultsConfig, more_specific: DefaultsConfig) -> DefaultsConfig {
    DefaultsConfig {
        isolation: more_specific.isolation.or(base.isolation),
        runner: more_specific.runner.or(base.runner),
        timeout_minutes: more_specific.timeout_minutes.or(base.timeout_minutes),
        on_failure: more_specific.on_failure.or(base.on_failure),
        max_parallel_nodes: more_specific.max_parallel_nodes.or(base.max_parallel_nodes),
        on_interrupt: more_specific.on_interrupt.or(base.on_interrupt),
    }
}

fn merge_limits_config(base: LimitsConfig, more_specific: LimitsConfig) -> LimitsConfig {
    LimitsConfig {
        max_tokens_per_run: more_specific.max_tokens_per_run.or(base.max_tokens_per_run),
        max_loop_iterations: more_specific
            .max_loop_iterations
            .or(base.max_loop_iterations),
        max_concurrent_runs: more_specific
            .max_concurrent_runs
            .or(base.max_concurrent_runs),
        max_workflow_depth: more_specific.max_workflow_depth.or(base.max_workflow_depth),
        max_expansion_files: more_specific
            .max_expansion_files
            .or(base.max_expansion_files),
        max_artifact_bytes: more_specific.max_artifact_bytes.or(base.max_artifact_bytes),
        inline_context_bytes: more_specific
            .inline_context_bytes
            .or(base.inline_context_bytes),
        max_artifact_repairs: more_specific
            .max_artifact_repairs
            .or(base.max_artifact_repairs),
    }
}

/// A map whose values are arrays: a key
/// present in the more specific layer replaces the base's value for that
/// key wholesale, rather than concatenating the two arrays.
fn merge_map_replacing_values<K, V>(
    base: Option<BTreeMap<K, Vec<V>>>,
    more_specific: Option<BTreeMap<K, Vec<V>>>,
) -> Option<BTreeMap<K, Vec<V>>>
where
    K: Ord,
{
    merge_maps(base, more_specific, |_base_value, override_value| {
        override_value
    })
}

/// A map whose values are themselves mergeable structs (field by field).
fn merge_map_of_fields<K, V>(
    base: Option<BTreeMap<K, V>>,
    more_specific: Option<BTreeMap<K, V>>,
    merge_value: impl Fn(V, V) -> V,
) -> Option<BTreeMap<K, V>>
where
    K: Ord,
{
    merge_maps(base, more_specific, merge_value)
}

fn merge_maps<K, V>(
    base: Option<BTreeMap<K, V>>,
    more_specific: Option<BTreeMap<K, V>>,
    merge_value: impl Fn(V, V) -> V,
) -> Option<BTreeMap<K, V>>
where
    K: Ord,
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
        adapter_settings: more_specific.adapter_settings.or(base.adapter_settings),
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
