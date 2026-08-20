//! The run manifest (T1.4, Contrato §2.1) — **M-0 cut**.
//!
//! `Manifest` freezes everything needed to interpret a run: the parsed
//! workflow, the merged config, the content of every file-referenced
//! prompt (§9.3: editing the file mid-run must not alter the run, I3)
//! and the base commit. Editing any source on disk after the freeze
//! changes nothing — the manifest is a value, and `manifest_hash` is a
//! pure function of it.
//!
//! Out of the M-0 cut, waiting for their schema to exist: `mode` (§10),
//! resolved `yunta_schema`, and `base_branch` (the `project:` config
//! group is deferred from T1.2). `inputs` itself landed in T1.5.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ConfigLayer, Isolation, NodeId, Workflow};

/// Absolute, fully-resolved state roots at run creation (T2.4/DI-07) —
/// post `YUNTA_HOME`, post config layers. Frozen so a later
/// `paths.runs`/`paths.worktrees` change can never lose a run that
/// already exists: everything after the manifest is found reads these,
/// never the current config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrozenPaths {
    pub runs_root: PathBuf,
    pub worktrees_root: PathBuf,
}

/// Everything a run needs frozen at creation time (Contrato §2.1). The
/// engine never re-reads workflow, config or prompt files during a run —
/// resume interprets the run with exactly what it was born with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Version of this manifest's own schema (D07: everything persisted
    /// is versioned from the first commit).
    pub schema_version: u32,
    /// Version of the yunta binary that created the run.
    pub yunta_version: String,
    pub workflow: Workflow,
    pub config: ConfigLayer,
    /// Every declared input resolved to its final string value — CLI-
    /// provided or the spec's own `default`, already validated (T1.5,
    /// §2.3). Frozen here so a node never resolves a default itself:
    /// that would be per-node non-deterministic state (D82).
    pub inputs: BTreeMap<String, String>,
    /// Content of every `prompt: {file: ...}` at freeze time, keyed by
    /// node id. Inline prompts are already frozen inside `workflow`.
    pub prompts: BTreeMap<NodeId, String>,
    /// Branch the run starts from (`git rev-parse --abbrev-ref HEAD` at
    /// creation; `"HEAD"` when detached).
    pub base_branch: String,
    /// Commit the run starts from (`git rev-parse HEAD` at creation).
    pub base_commit: String,
    /// Resolved `defaults.isolation` (§7.3, T4.2) — a run's own mode
    /// never changes after creation, even if config does.
    pub isolation: Isolation,
    /// Resolved `defaults.max_parallel_nodes` (T4.1) — how many
    /// independently-ready DAG nodes the scheduler may run at once for
    /// this run, frozen the same way as `isolation`.
    pub max_parallel_nodes: u32,
    pub workflow_hash: String,
    pub config_hash: String,
    /// `None` on manifests written before DI-07 (tolerant reader, D70):
    /// those fall back to the current config's paths — exactly the
    /// pre-freeze behavior, so old runs stay resumable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<FrozenPaths>,
}

impl Manifest {
    /// Hash of the whole frozen manifest — the value `run_created`
    /// records, and the genesis of the event hash chain (§3.3, not
    /// implemented in M-0).
    pub fn manifest_hash(&self) -> String {
        content_hash(self)
    }
}

/// SHA-256 over a canonical JSON rendering of any serializable value:
/// object keys sorted, no insignificant whitespace. Canonicalization is
/// done here explicitly instead of trusting the serializer's map order,
/// so the hash can never silently depend on a feature flag or on
/// `HashMap` iteration order.
pub fn content_hash<T: Serialize>(value: &T) -> String {
    // Serializing our own plain-data types into a JSON value cannot fail
    // (no non-string map keys, no non-serializable leaves); a change that
    // breaks this breaks every manifest test immediately.
    let value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
    let mut canonical = String::new();
    write_canonical(&value, &mut canonical);

    sha256_hex(canonical.as_bytes())
}

/// Lowercase-hex SHA-256 of raw bytes — what `artifact_written` records
/// for a file's content (§4: artifacts are verified by existence and
/// hash, never by format).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
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
