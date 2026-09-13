//! The JSON Schema (draft 2020-12) of every document a person or a tool
//! reads and writes, generated from the types that parse them — the
//! types are the source of truth, the schema their emission. The
//! repository keeps the rendered files under `crates/core/schemas/`,
//! written by `cargo xtask schema` and checked by CI, so a change to a
//! format is a visible diff — and so a command that publishes a schema
//! embeds the checked file rather than generating one, which would put
//! the whole of `schemars` in the shipped binary.

use schemars::schema_for;
pub use schemars::Schema;

/// The workflow file: `.yunta/workflows/<name>.yaml`.
pub fn workflow() -> Schema {
    titled(schema_for!(crate::Workflow), "yunta workflow")
}

/// One config layer: `.yunta/config.yaml` and its user and org twins.
pub fn config() -> Schema {
    titled(schema_for!(crate::ConfigLayer), "yunta config layer")
}

/// A pack manifest: `pack.yaml`.
pub fn pack() -> Schema {
    titled(schema_for!(crate::PackManifest), "yunta pack manifest")
}

/// A tasks document, as a `kind: tasks` artifact carries it.
pub fn tasks() -> Schema {
    titled(schema_for!(crate::TasksFile), "yunta tasks")
}

/// A findings artifact, as a `kind: findings` artifact carries it.
pub fn findings() -> Schema {
    titled(schema_for!(crate::FindingsFile), "yunta findings")
}

/// A questions artifact, as a `kind: questions` artifact carries it.
pub fn questions() -> Schema {
    titled(schema_for!(crate::QuestionsFile), "yunta questions")
}

/// A withdrawal, as `yunta_withdraw_finding` receives it.
pub fn withdrawal() -> Schema {
    titled(schema_for!(crate::Withdrawal), "yunta finding withdrawal")
}

/// One stored event — a line of `events.jsonl`: the envelope and the
/// payload of its kind, side by side.
pub fn events() -> Schema {
    titled(schema_for!(crate::events::StoredEvent), "yunta event")
}

/// Every root schema with the file name it is kept under. A kind the
/// engine parses and validates is a kind whose schema it publishes, so
/// the three interpreted artifact kinds are all here.
pub fn all() -> [(&'static str, Schema); 8] {
    [
        ("workflow", workflow()),
        ("config", config()),
        ("pack", pack()),
        ("tasks", tasks()),
        ("findings", findings()),
        ("questions", questions()),
        ("withdrawal", withdrawal()),
        ("events", events()),
    ]
}

fn titled(mut schema: Schema, title: &str) -> Schema {
    schema.insert("title".to_string(), title.into());
    schema
}

/// The published JSON Schema of one interpreted document, as the
/// repository keeps it.
///
/// The committed bytes rather than a freshly generated schema:
/// `cargo xtask schema --check` is what holds the two together, and
/// embedding the result keeps schema generation — and all of
/// `schemars` — out of the shipped binary.
pub fn json(kind: crate::ArtifactKind) -> &'static str {
    match kind {
        crate::ArtifactKind::Tasks => include_str!("../schemas/tasks.json"),
        crate::ArtifactKind::Findings => include_str!("../schemas/findings.json"),
        crate::ArtifactKind::Questions => include_str!("../schemas/questions.json"),
    }
}
