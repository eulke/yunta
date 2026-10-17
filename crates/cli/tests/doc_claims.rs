//! The design documents are held to the code and the schema they describe.
//! A claim a document makes about a closed set, a field or an open item
//! fails here once the binary publishes something else. This file reads
//! the documents by name because they are its subject.

use std::path::PathBuf;

use yunta_testkit::{
    enum_variants, fenced_blocks, has_top_level_key, json_schema, number_before, section,
    trait_methods,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn reference_schema() -> PathBuf {
    repo_root().join("docs/design/referencia-schema.md")
}

/// One of the design documents, by file name.
fn design_doc(name: &str) -> String {
    let path = repo_root().join("docs/design").join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read `{}`: {e}", path.display()))
}

/// The config layer's example in the reference schema: the first YAML block
/// that declares `limits`.
fn reference_config() -> String {
    fenced_blocks(&reference_schema(), "yaml")
        .into_iter()
        .find(|b| has_top_level_key(&b.text, "limits"))
        .expect("config")
        .text
}

/// The payload spec's section 5 states how many kinds the log writes: the
/// count must be the length of the list the binary publishes.
#[test]
fn the_payload_spec_heading_counts_the_kinds_the_binary_writes() {
    let spec = design_doc("spec-events.md");
    let heading = spec
        .lines()
        .find(|l| l.starts_with("## 5."))
        .expect("section 5");
    assert_eq!(
        number_before(heading, "tipos"),
        Some(yunta_core::events::EventPayload::KINDS.len())
    );
}

/// The adapter spec's event section lists exactly the variants of
/// `AgentEvent` the port declares.
#[test]
fn the_adapter_spec_lists_every_agent_event() {
    let port =
        std::fs::read_to_string(repo_root().join("crates/core/src/port/session.rs")).unwrap();
    let spec = section(&design_doc("spec-adapter.md"), "## 3.");
    assert_eq!(
        enum_variants(&spec, "AgentEvent"),
        enum_variants(&port, "AgentEvent")
    );
}

/// The adapter spec's trait section lists exactly the methods of `Adapter`
/// and `AgentSession` the port declares.
#[test]
fn the_adapter_spec_lists_every_adapter_method() {
    let port =
        std::fs::read_to_string(repo_root().join("crates/core/src/port/session.rs")).unwrap();
    let spec = section(&design_doc("spec-adapter.md"), "## 2.");
    for name in ["Adapter", "AgentSession"] {
        assert_eq!(
            trait_methods(&spec, name),
            trait_methods(&port, name),
            "{name}"
        );
    }
}

/// Every `on_interrupt` value the workflow schema accepts appears in the
/// reference schema, so a value the schema takes is one a reader can find.
#[test]
fn the_reference_names_every_on_interrupt_value() {
    let schema = json_schema(&repo_root().join("crates/core/schemas/workflow.json"));
    let reference = design_doc("referencia-schema.md");
    for alt in schema["$defs"]["OnInterrupt"]["oneOf"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let values = alt["enum"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![alt["const"].clone()]);
        for value in values.iter().filter_map(|v| v.as_str()) {
            assert!(reference.contains(value), "{value}");
        }
    }
}

/// Every top-level key the config schema declares appears in the reference
/// config.
#[test]
fn the_reference_names_every_config_layer_key() {
    let config = reference_config();
    let schema = json_schema(&repo_root().join("crates/core/schemas/config.json"));
    for key in schema["properties"].as_object().expect("keys").keys() {
        assert!(has_top_level_key(&config, key), "{key}");
    }
}

/// Every field of every permission group the config schema declares appears
/// in the reference config's `permissions` block.
#[test]
fn the_reference_names_every_permission_key() {
    let example: serde_json::Value =
        yunta_core::yaml::parse(&reference_config()).expect("the reference config parses");
    let schema = json_schema(&repo_root().join("crates/core/schemas/config.json"));
    let defs = &schema["$defs"];
    for (group, prop) in defs["PermissionsConfig"]["properties"]
        .as_object()
        .expect("groups")
    {
        let target = prop["anyOf"][0]["$ref"]
            .as_str()
            .expect("ref")
            .trim_start_matches("#/$defs/");
        let Some(block) = example["permissions"].get(group) else {
            panic!("permissions.{group} is missing from the reference config");
        };
        for field in defs[target]["properties"]
            .as_object()
            .expect("fields")
            .keys()
        {
            assert!(
                block.get(field).is_some(),
                "permissions.{group}.{field} is missing from the reference config"
            );
        }
    }
}

/// The open section of the debt register names no item as closed. The
/// capital letter keeps the lowercase form of the closed-item word, which an
/// open item's prose uses, from matching.
#[test]
fn the_open_debt_names_no_closed_item() {
    assert!(!section(&design_doc("deuda-consciente.md"), "## Abiertos").contains("Cerrad"));
}
