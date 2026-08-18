//! Serde types for the workflow schema (T1.1) — **M-0 cut only**.
//!
//! The full T1.1 (M1) covers every node `kind`, `runners:`/`agent:`
//! resolution, `context:`, `skills:`, `modes:`, `inputs:`, gates and
//! composition. M-0's "schema recortado" (Plan, sección M-0) is
//! deliberately narrower: only `prompt`/`bash`/`loop` nodes, `depends_on`,
//! `scope`, `artifacts`, `hooks: {before, after}` and `on_failure.goto` —
//! just enough for the implement → compile → correct cycle the bootstrap
//! needs to validate. Every other workflow field is out of scope here and
//! will extend these types when its own task lands (T1.1 proper, in true
//! M1 execution), not before.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::NodeId;

/// A workflow definition (Contrato §2, §10 — minus modes/inputs, out of
/// scope for M-0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub nodes: Vec<Node>,
}

/// A single node. Fields here are the ones the M-0 recorte names
/// explicitly; `runner` is included because a `prompt`/`loop` node is
/// meaningless without picking a runner, even though the resolution
/// mechanism itself (`runners:` candidates, capability probing, I17) is
/// config-layer work for a later task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    #[serde(flatten)]
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<NodeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Artifacts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Hooks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<OnFailure>,
}

/// The three node kinds M-0 needs (Plan, sección M-0). `gate`, `check`,
/// `parallel`, `executor` and `workflow` are the rest of the full T1.1
/// catalogue and stay out until their own milestone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeKind {
    Prompt { prompt: PromptSource },
    Bash { run: String },
    Loop { until: String, prompt: PromptSource },
}

/// A node's prompt: an inline string, or `{file: ...}` (Contrato §9.3,
/// D78). The distinction is **structural, never heuristic**: a scalar is
/// always literal text, even if its contents look like a path — only an
/// explicit `{file: ...}` mapping reads from disk.
#[derive(Debug, Clone, PartialEq)]
pub enum PromptSource {
    Inline(String),
    File(std::path::PathBuf),
}

impl<'de> Deserialize<'de> for PromptSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Scalar(String),
            File { file: std::path::PathBuf },
        }

        match Raw::deserialize(deserializer)? {
            Raw::Scalar(text) => Ok(PromptSource::Inline(text)),
            Raw::File { file } => Ok(PromptSource::File(file)),
        }
    }
}

impl Serialize for PromptSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;

        match self {
            PromptSource::Inline(text) => serializer.serialize_str(text),
            PromptSource::File(path) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("file", path)?;
                map.end()
            }
        }
    }
}

/// `artifacts.produces` (Contrato §4). M-0 only needs `kind: task-ledger`
/// interpreted — `findings`/`questions` belong to gates/review flows that
/// are out of scope; a plain string stays an opaque artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifacts {
    pub produces: Vec<ArtifactSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArtifactSpec {
    Plain(String),
    Typed { name: String, kind: ArtifactKind },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    TaskLedger,
}

/// `hooks: {before, after}` (D81). Per-hook `timeout`/`on_failure` are
/// T4.3 (M4) engine behavior, not schema needed for M-0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hooks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<HookStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<HookStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookStep {
    pub run: String,
}

/// `on_failure: {goto, max_reroutes}` — node-level re-routing (§11.2).
/// `max_reroutes` is mandatory (D24): a re-route without an explicit cap
/// is how a correction cycle turns infinite, so the schema refuses it.
/// Distinct from a hook's own `on_failure: fail|warn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnFailure {
    pub goto: NodeId,
    pub max_reroutes: u32,
}
