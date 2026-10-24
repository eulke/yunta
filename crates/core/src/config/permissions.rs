//! `permissions:` — the one ceiling in the config: the org layer bounds,
//! lower layers only narrow, and a loosening attempt is reported by name.

use serde::{Deserialize, Serialize};

use super::ConfigLayer;
use crate::ids::Publisher;

/// `permissions:` — ONE model of ceilings, not
/// loose mechanisms: each level may only narrow the one above, never
/// loosen it. Unlike every other config group (repo > user > org), the
/// org layer rules here and lower layers only restrict further — without
/// that inversion, governance is theater: any repo could undo it.
///
/// This is governance, not a sandbox: an agent
/// with write access can route around a textual pattern by writing a
/// script and running it. The model stops the accident and the careless
/// pack, and leaves an auditable trail of the deliberate attempt — real
/// isolation belongs to the execution environment, never to Yunta.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PermissionsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<CommandPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packs: Option<PackPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_expansion: Option<ScopeExpansionPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<PathPermissions>,
}

/// `permissions.paths` — what no node and no task of any run may write:
/// the project's CI, the configuration of its own checks, whatever the
/// team decides no agent touches. A ceiling like the rest: every layer's
/// denies stand, and none is ever granted back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathPermissions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<crate::ScopeGlob>,
}

/// `permissions.scope_expansion`: the layered ceiling
/// over how a loop node may let its tasks grow past declared scope.
/// `max_mode` is the most *permissive* node-level `scope_expansion.mode`
/// the layer allows (`rules < ask < deny` in severity) — the same
/// only-narrowing model every other `permissions` group follows:
/// merge keeps the strictest declared ceiling, a lower layer softening
/// it is a reported conflict, and a node declaring a mode over the
/// merged ceiling fails `check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopeExpansionPermissions {
    pub max_mode: crate::policy::ScopeCeiling,
}

/// `permissions.commands` — patterns matched against every hook, criterion,
/// bash node and executor command right before it runs. Empty
/// `allow` = denylist mode (everything not denied runs); a non-empty
/// `allow` switches to a strict, opt-in allowlist.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandPermissions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
}

/// `permissions.packs` — governance over pack contents. Parsed and
/// merged here; enforced by `pack add` and `pack update`, which read the
/// merged value and name the layer that declared it when they refuse.
///
/// Two settings and no matrix: which publishers may be installed, and
/// whether a pack's executable code may run. Trusting one publisher's
/// executors and not another's is the allow-list with `executors:
/// prompt`, which asks case by case.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackPermissions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executors: Option<PackExecutorPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publishers: Option<PublisherPermissions>,
}

/// `allow | prompt | deny`, strictly ordered: `Deny` is the narrowest,
/// `Allow` the loosest — the ceiling merge keeps the strictest across
/// layers. `prompt` asks for confirmation at `yunta pack add`,
/// never mid-run: runs are headless, humans interact through gates only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackExecutorPolicy {
    Allow,
    Prompt,
    Deny,
}

impl PackExecutorPolicy {
    /// The value as a config file writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            PackExecutorPolicy::Allow => "allow",
            PackExecutorPolicy::Prompt => "prompt",
            PackExecutorPolicy::Deny => "deny",
        }
    }

    fn strictness(self) -> u8 {
        match self {
            PackExecutorPolicy::Allow => 0,
            PackExecutorPolicy::Prompt => 1,
            PackExecutorPolicy::Deny => 2,
        }
    }
}

/// `permissions.packs.publishers` — `allow` empty means every publisher
/// is accepted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublisherPermissions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<Publisher>,
}

/// `permissions.network` — whether the commands an agent's session runs
/// may reach the network when its node says nothing: the session's CLI
/// keeps it in its own sandbox. For any other command it is declarative;
/// an executor that wants to enforce it does so on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkPermissions {
    pub default: bool,
}

/// Ceiling merge for `permissions`: the effective model is the
/// most restrictive combination of both layers, computed conservatively —
/// even when a lower layer *tried* to loosen (a check error via
/// [`permission_layer_conflicts`]), the runtime model never runs anything
/// the ceiling denied.
pub(super) fn merge_permissions(
    ceiling: Option<PermissionsConfig>,
    lower: Option<PermissionsConfig>,
) -> Option<PermissionsConfig> {
    let (ceiling, lower) = match (ceiling, lower) {
        (None, None) => return None,
        (Some(c), None) => return Some(c),
        (None, Some(l)) => return Some(l),
        (Some(c), Some(l)) => (c, l),
    };

    Some(PermissionsConfig {
        commands: merge_group(ceiling.commands, lower.commands, merge_commands),
        packs: merge_group(ceiling.packs, lower.packs, merge_packs),
        network: merge_group(ceiling.network, lower.network, |c, l| NetworkPermissions {
            // `false` is the narrower value — a ceiling that turned the
            // default off stays off no matter what a lower layer says.
            default: c.default && l.default,
        }),
        // Ceiling semantics, same as packs: the strictest declared wins.
        scope_expansion: merge_group(ceiling.scope_expansion, lower.scope_expansion, |c, l| {
            ScopeExpansionPermissions {
                max_mode: c.max_mode.stricter(l.max_mode),
            }
        }),
        // Union, like `commands.deny`: a lower layer only ever adds a path.
        paths: merge_group(ceiling.paths, lower.paths, merge_paths),
    })
}

/// One group of `permissions` across both layers: a group only one layer
/// names stands as written, and a group both name merges by `both`.
fn merge_group<T>(ceiling: Option<T>, lower: Option<T>, both: impl FnOnce(T, T) -> T) -> Option<T> {
    match (ceiling, lower) {
        (Some(c), Some(l)) => Some(both(c, l)),
        (c, l) => c.or(l),
    }
}

/// Commands: denies union, and the ceiling's non-empty allow bounds the
/// lower layer's.
fn merge_commands(ceiling: CommandPermissions, lower: CommandPermissions) -> CommandPermissions {
    // Denies union: adding denies is narrowing, always legal.
    let mut deny = ceiling.deny;
    for pattern in lower.deny {
        if !deny.contains(&pattern) {
            deny.push(pattern);
        }
    }
    // Allows: the ceiling's non-empty allow bounds the lower
    // layer's — entries outside it are dropped here (and reported
    // as conflicts by the checker, not swallowed silently).
    let allow = if ceiling.allow.is_empty() {
        lower.allow
    } else if lower.allow.is_empty() {
        ceiling.allow
    } else {
        lower
            .allow
            .into_iter()
            .filter(|pattern| ceiling.allow.contains(pattern))
            .collect()
    };
    CommandPermissions { deny, allow }
}

/// Packs: the strictest executor policy wins, and publishers intersect.
fn merge_packs(ceiling: PackPermissions, lower: PackPermissions) -> PackPermissions {
    PackPermissions {
        executors: merge_group(ceiling.executors, lower.executors, |a, b| {
            if a.strictness() >= b.strictness() {
                a
            } else {
                b
            }
        }),
        publishers: merge_group(ceiling.publishers, lower.publishers, merge_publishers),
    }
}

/// Publishers: empty means everyone, so a non-empty ceiling bounds the
/// lower list, and two non-empty lists intersect.
fn merge_publishers(
    ceiling: PublisherPermissions,
    lower: PublisherPermissions,
) -> PublisherPermissions {
    let allow = if ceiling.allow.is_empty() {
        lower.allow
    } else if lower.allow.is_empty() {
        ceiling.allow
    } else {
        lower
            .allow
            .into_iter()
            .filter(|publisher| ceiling.allow.contains(publisher))
            .collect()
    };
    PublisherPermissions { allow }
}

/// Paths: the union of both layers' denies, so a lower layer only adds.
fn merge_paths(ceiling: PathPermissions, lower: PathPermissions) -> PathPermissions {
    let mut deny = ceiling.deny;
    for glob in lower.deny {
        if !deny.contains(&glob) {
            deny.push(glob);
        }
    }
    PathPermissions { deny }
}

/// Whether a session's commands may reach the network: the node's word
/// when it gives one, else the project's default, else open.
pub fn network_asked(node: Option<bool>, project: Option<NetworkPermissions>) -> bool {
    node.or(project.map(|network| network.default))
        .unwrap_or(true)
}

/// Detects loosening attempts across ordered permission layers — the
/// case this guards is a repo layer trying to re-allow a pattern the org
/// layer denied: `check` rejects it, citing the offending layer.
/// `layers` come
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
                            "layer `{lower_name}` re-allows command pattern `{pattern}` denied by layer `{higher_name}` — permissions only narrow"
                        ));
                    } else if !higher_cmds.allow.is_empty() && !higher_cmds.allow.contains(pattern)
                    {
                        conflicts.push(format!(
                            "layer `{lower_name}` allows command pattern `{pattern}` outside layer `{higher_name}`'s allowlist — permissions only narrow"
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
                            "layer `{lower_name}` loosens `packs.executors` to `{}` below layer `{higher_name}`'s `{}` — permissions only narrow",
                            lower_pol.as_str(),
                            higher_pol.as_str()
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
                                    "layer `{lower_name}` allows publisher `{publisher}` outside layer `{higher_name}`'s allowlist — permissions only narrow"
                                ));
                            }
                        }
                    }
                }
            }

            if let (Some(lower_net), Some(higher_net)) = (lower.network, higher.network) {
                if lower_net.default && !higher_net.default {
                    conflicts.push(format!(
                        "layer `{lower_name}` re-enables `network.default` turned off by layer `{higher_name}` — permissions only narrow"
                    ));
                }
            }

            if let (Some(lower_se), Some(higher_se)) =
                (lower.scope_expansion, higher.scope_expansion)
            {
                if !higher_se.max_mode.allows(lower_se.max_mode.0) {
                    conflicts.push(format!(
                        "layer `{lower_name}` softens `scope_expansion.max_mode` to `{}` below layer `{higher_name}`'s `{}` — permissions only narrow",
                        lower_se.max_mode.0.as_str(),
                        higher_se.max_mode.0.as_str()
                    ));
                }
            }
        }
    }

    conflicts
}
