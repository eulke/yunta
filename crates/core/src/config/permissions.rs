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
    pub max_mode: crate::policy::ScopeExpansionMode,
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

/// `permissions.packs` — governance over pack contents. Parsed
/// and merged here; *enforced* at `pack add`/check once pack support
/// lands fully — a key without its consumer yet, kept
/// because the org ceiling file is one document and its schema shouldn't
/// dribble in piecemeal.
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

/// `permissions.network` — declarative ONLY: `default: false`
/// activates no sandboxing whatsoever. It exists for policy and audit; an
/// executor that wants to actually enforce it does so on its own. Policy
/// ≠ capability ≠ OS enforcement — Yunta core never promises the third.
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
                    // Empty = everyone — a non-empty ceiling bounds
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

    // Ceiling semantics, same as packs: the strictest declared wins.
    let scope_expansion = match (ceiling.scope_expansion, lower.scope_expansion) {
        (Some(c), Some(l)) => Some(if c.max_mode.strictness() >= l.max_mode.strictness() {
            c
        } else {
            l
        }),
        (c, l) => c.or(l),
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
        scope_expansion,
    })
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
                            "layer `{lower_name}` loosens `packs.executors` to `{lower_pol:?}` below layer `{higher_name}`'s `{higher_pol:?}` — permissions only narrow"
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
                if lower_se.max_mode.strictness() < higher_se.max_mode.strictness() {
                    conflicts.push(format!(
                        "layer `{lower_name}` softens `scope_expansion.max_mode` to `{}` below layer `{higher_name}`'s `{}` — permissions only narrow",
                        lower_se.max_mode.as_str(),
                        higher_se.max_mode.as_str()
                    ));
                }
            }
        }
    }

    conflicts
}
