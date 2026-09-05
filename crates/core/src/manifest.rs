//! The run manifest.
//!
//! `Manifest` freezes everything needed to interpret a run: the parsed
//! workflow, the merged config, the content of every file-referenced
//! prompt (editing the file mid-run must not alter the run)
//! and the base commit. Editing any source on disk after the freeze
//! changes nothing — the manifest is a value, and `manifest_hash` is a
//! pure function of it.
//!
//! Still waiting for their schema to exist: `mode`,
//! resolved `yunta_schema`, and `base_branch` (the `project:` config
//! group isn't wired through yet).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hash::{sha256_hex, CommitSha, ContentHash};
use crate::{ConfigLayer, Isolation, NodeId, PackName, Publisher, Workflow};

/// The state roots a run is frozen to at creation — post `YUNTA_HOME`,
/// post config layers. Absolute by construction ([`FrozenPaths::new`] is
/// the only way to author one): everything after the manifest is found
/// reads these back from whatever directory the reader runs in
/// (`resume`, `status`, `gc`, a detached child), so a relative root would
/// resolve against the wrong place. Frozen so a later
/// `paths.runs`/`paths.worktrees` change can never lose a run that
/// already exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FrozenPaths {
    runs_root: PathBuf,
    worktrees_root: PathBuf,
}

/// A state root that cannot be frozen because it is not absolute. Named
/// so the person who set the offending `paths.*` / `YUNTA_HOME` sees which
/// one to fix — the same shape [`HomeExpansionError`](crate::HomeExpansionError)
/// takes for a config path that cannot be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the {field} `{path}` is not absolute — a run freezes its state roots at creation and \
     reads them back from any directory, so a relative one would resolve against the wrong \
     place later; give an absolute path"
)]
pub struct RelativeRootError {
    pub field: &'static str,
    pub path: PathBuf,
}

impl FrozenPaths {
    /// Freezes the two state roots, refusing either that is not absolute
    /// with an error naming it. The single constructor, so no authored
    /// manifest can carry a relative root a later reader would resolve
    /// against its own directory.
    pub fn new(runs_root: PathBuf, worktrees_root: PathBuf) -> Result<Self, RelativeRootError> {
        if !runs_root.is_absolute() {
            return Err(RelativeRootError {
                field: "runs root",
                path: runs_root,
            });
        }
        if !worktrees_root.is_absolute() {
            return Err(RelativeRootError {
                field: "worktrees root",
                path: worktrees_root,
            });
        }
        Ok(Self {
            runs_root,
            worktrees_root,
        })
    }

    /// The absolute root every run.dir of this run's tree sits under.
    pub fn runs_root(&self) -> &Path {
        &self.runs_root
    }

    /// The absolute root every isolated worktree of this run's tree sits
    /// under.
    pub fn worktrees_root(&self) -> &Path {
        &self.worktrees_root
    }
}

/// Which pack (and exactly which version of it) a run's top-level
/// workflow came from — frozen the same moment
/// `workflow` itself is: `update`-ing the pack afterward can't touch a
/// run already born, since resume only ever re-reads this manifest,
/// never the vendored pack on disk again. `commit` is `None` when the
/// pack was vendored without an accompanying `yunta.lock` entry (hand-
/// placed rather than through `pack add`) — `version` alone still
/// identifies it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PackProvenance {
    pub publisher: Publisher,
    pub name: PackName,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

/// Everything a run needs frozen at creation time. The
/// engine never re-reads workflow, config or prompt files during a run —
/// resume interprets the run with exactly what it was born with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Manifest {
    /// Version of this manifest's own schema — everything persisted
    /// is versioned from the first commit.
    pub schema_version: u32,
    /// Version of the yunta binary that created the run.
    pub yunta_version: String,
    pub workflow: Workflow,
    pub config: ConfigLayer,
    /// Every declared input resolved to its final string value — CLI-
    /// provided or the spec's own `default`, already validated.
    /// Frozen here so a node never resolves a default itself:
    /// that would be per-node non-deterministic state.
    pub inputs: BTreeMap<String, String>,
    /// Content of every `prompt: {file: ...}` at freeze time, keyed by
    /// node id. Inline prompts are already frozen inside `workflow`.
    pub prompts: BTreeMap<NodeId, String>,
    /// Branch the run starts from (`git rev-parse --abbrev-ref HEAD` at
    /// creation; `"HEAD"` when detached).
    pub base_branch: String,
    /// Commit the run starts from (`git rev-parse HEAD` at creation).
    pub base_commit: CommitSha,
    /// Resolved `defaults.isolation` — a run's own mode
    /// never changes after creation, even if config does.
    pub isolation: Isolation,
    /// Resolved `defaults.max_parallel_nodes` — how many
    /// independently-ready DAG nodes the scheduler may run at once for
    /// this run, frozen the same way as `isolation`.
    pub max_parallel_nodes: u32,
    pub workflow_hash: ContentHash,
    pub config_hash: ContentHash,
    /// `None` on manifests written before this field existed (tolerant
    /// reader): those fall back to the current config's paths — exactly the
    /// pre-freeze behavior, so old runs stay resumable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<FrozenPaths>,
    /// `None` for a repo-origin workflow, or for a manifest written
    /// before pack support existed (tolerant reader).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackProvenance>,
}

impl Manifest {
    /// Hash of the whole frozen manifest — the value `run_created`
    /// records, and what the storage derives the event hash chain's
    /// genesis from (`H0 = SHA-256(manifest_hash)`): one chain per run,
    /// anchored in the run's own frozen inputs rather than a constant.
    pub fn manifest_hash(&self) -> ContentHash {
        content_hash(self)
    }
}

/// SHA-256 over a canonical JSON rendering of any serializable value:
/// object keys sorted, no insignificant whitespace. Canonicalization is
/// done here explicitly instead of trusting the serializer's map order,
/// so the hash can never silently depend on a feature flag or on
/// `HashMap` iteration order.
pub fn content_hash<T: Serialize>(value: &T) -> ContentHash {
    // Serializing our own plain-data types into a JSON value cannot fail
    // (no non-string map keys, no non-serializable leaves); a change that
    // breaks this breaks every manifest test immediately.
    let value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
    let mut canonical = String::new();
    write_canonical(&value, &mut canonical);

    sha256_hex(canonical.as_bytes())
}

fn write_canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::Value::String((*key).clone()).to_string());
                out.push(':');
                write_canonical(&map[*key], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        leaf => out.push_str(&leaf.to_string()),
    }
}
