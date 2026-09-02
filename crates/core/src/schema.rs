//! The JSON Schema (draft 2020-12) of every document a person or a tool
//! reads and writes, generated from the types that parse them — the
//! types are the source of truth, the schema their emission. The
//! repository keeps the rendered files under `schemas/`, written by
//! `cargo xtask schema` and checked by CI, so a change to a format is
//! a visible diff.

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

/// A task ledger, as a `kind: task-ledger` artifact carries it.
pub fn ledger() -> Schema {
    titled(schema_for!(crate::Ledger), "yunta task ledger")
}

/// One stored event — a line of `events.jsonl`: the envelope and the
/// payload of its kind, side by side.
pub fn events() -> Schema {
    titled(schema_for!(crate::events::StoredEvent), "yunta event")
}

/// Every root schema with the file name it is kept under.
pub fn all() -> [(&'static str, Schema); 5] {
    [
        ("workflow", workflow()),
        ("config", config()),
        ("pack", pack()),
        ("ledger", ledger()),
        ("events", events()),
    ]
}

fn titled(mut schema: Schema, title: &str) -> Schema {
    schema.insert("title".to_string(), title.into());
    schema
}
