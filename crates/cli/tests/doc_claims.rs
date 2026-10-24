//! The documents are held to the code and the schema they describe. A
//! claim a document makes about a closed set, a field or an open item
//! fails here once the binary publishes something else, and the reference
//! shows every key the config and the workflow schemas publish. This file
//! reads the documents by name because they are its subject.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::Value;
use yunta_testkit::{
    enum_variants, fenced_blocks, has_top_level_key, json_schema, markdown_files, number_before,
    section, trait_methods,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The reference: every key of the config and the workflow language, each
/// with a value.
fn reference() -> PathBuf {
    repo_root().join("docs/reference.md")
}

fn reference_text() -> String {
    let path = reference();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read `{}`: {e}", path.display()))
}

/// One of the design documents, by file name.
fn design_doc(name: &str) -> String {
    let path = repo_root().join("docs/design").join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read `{}`: {e}", path.display()))
}

/// The config layer's example in the reference: the first YAML block that
/// declares `limits`.
fn reference_config() -> String {
    fenced_blocks(&reference(), "yaml")
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
/// reference, so a value the schema takes is one a reader can find.
#[test]
fn the_reference_names_every_on_interrupt_value() {
    let schema = json_schema(&repo_root().join("crates/core/schemas/workflow.json"));
    let reference = reference_text();
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

/// Every key path of the config schema, through each group, each map's
/// entries and each list's items, is one the reference config shows with a
/// value.
#[test]
fn the_reference_names_every_config_layer_key() {
    let example: Value =
        yunta_core::yaml::parse(&reference_config()).expect("the reference config parses");
    let schema = json_schema(&repo_root().join("crates/core/schemas/config.json"));
    let mut paths = Vec::new();
    key_paths(&schema, &schema, &mut Vec::new(), &mut paths);
    assert!(
        paths.len() > 50,
        "the schema publishes {} keys",
        paths.len()
    );
    for path in &paths {
        assert!(
            shows(&example, path),
            "the reference config shows no `{}` — give it a value there",
            path.join(".")
        );
    }
}

/// Every key the workflow schema declares is one a workflow or a node the
/// reference shows uses with a value.
#[test]
fn the_reference_names_every_workflow_key() {
    let schema = json_schema(&repo_root().join("crates/core/schemas/workflow.json"));
    let mut declared = BTreeSet::new();
    property_names(&schema, &mut declared);
    let mut used = BTreeSet::new();
    for block in fenced_blocks(&reference(), "yaml") {
        let shown: Value = yunta_core::yaml::parse(&block.text)
            .unwrap_or_else(|e| panic!("{}: {e}", block.origin));
        keys_of(&shown, &mut used);
    }
    let missing: Vec<&String> = declared.difference(&used).collect();
    assert!(
        missing.is_empty(),
        "the reference shows no workflow with {missing:?} — give each a value in a block"
    );
}

/// Every template variable is named in the reference, braces and all.
#[test]
fn the_reference_names_every_template_variable() {
    let reference = reference_text();
    for variable in yunta_core::template::TemplateVar::FIXED {
        assert!(
            reference.contains(&variable.braced()),
            "the reference names no `{}`",
            variable.braced()
        );
    }
    assert!(reference.contains("{{inputs.<name>}}"));
}

/// Every input type the workflow schema accepts is named in the reference.
#[test]
fn the_reference_names_every_input_type() {
    let reference = reference_text();
    let schema = json_schema(&repo_root().join("crates/core/schemas/workflow.json"));
    let types: Vec<&str> = schema["$defs"]["InputSpec"]["oneOf"]
        .as_array()
        .expect("input types")
        .iter()
        .filter_map(|shape| shape["properties"]["type"]["const"].as_str())
        .collect();
    assert_eq!(types.len(), 6, "{types:?}");
    for name in types {
        assert!(
            reference.contains(&format!("`{name}`")),
            "the reference names no input type `{name}`"
        );
    }
}

/// `node` with its references followed: the definition a `$ref` names.
fn followed<'a>(root: &'a Value, node: &'a Value) -> &'a Value {
    match node["$ref"]
        .as_str()
        .and_then(|r| r.strip_prefix("#/$defs/"))
    {
        Some(name) => &root["$defs"][name],
        None => node,
    }
}

/// The key paths under `node`: each property's name, `*` for a map's
/// entries and `[]` for a list's items.
fn key_paths(root: &Value, node: &Value, at: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    let node = followed(root, node);
    if at.len() > 8 {
        return;
    }
    for alternatives in ["anyOf", "oneOf", "allOf"] {
        for branch in node[alternatives].as_array().into_iter().flatten() {
            key_paths(root, branch, at, out);
        }
    }
    for (name, field) in node["properties"].as_object().into_iter().flatten() {
        at.push(name.clone());
        out.push(at.clone());
        key_paths(root, field, at, out);
        at.pop();
    }
    for (marker, inner) in [("*", &node["additionalProperties"]), ("[]", &node["items"])] {
        if inner.is_object() {
            at.push(marker.to_string());
            key_paths(root, inner, at, out);
            at.pop();
        }
    }
}

/// Whether `value` has something at `path`, a `*` matching any entry of a
/// map and a `[]` any item of a list.
fn shows(value: &Value, path: &[String]) -> bool {
    let Some((head, rest)) = path.split_first() else {
        return true;
    };
    match head.as_str() {
        "*" => value
            .as_object()
            .is_some_and(|map| map.values().any(|entry| shows(entry, rest))),
        "[]" => value
            .as_array()
            .is_some_and(|items| items.iter().any(|item| shows(item, rest))),
        key => value.get(key).is_some_and(|inner| shows(inner, rest)),
    }
}

/// Every property name a schema declares, in any of its definitions.
fn property_names(node: &Value, names: &mut BTreeSet<String>) {
    match node {
        Value::Object(map) => {
            if let Some(properties) = map.get("properties").and_then(Value::as_object) {
                names.extend(properties.keys().cloned());
            }
            map.values().for_each(|inner| property_names(inner, names));
        }
        Value::Array(items) => items.iter().for_each(|inner| property_names(inner, names)),
        _ => {}
    }
}

/// Every key a YAML value uses, at any depth.
fn keys_of(value: &Value, keys: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            keys.extend(map.keys().cloned());
            map.values().for_each(|inner| keys_of(inner, keys));
        }
        Value::Array(items) => items.iter().for_each(|inner| keys_of(inner, keys)),
        _ => {}
    }
}

/// The reference shows the config and the workflows the core suite parses
/// byte for byte: one source, so what a reader copies is what the types
/// read, comments and all.
#[test]
fn the_reference_shows_the_files_the_core_suite_parses() {
    let shown: Vec<String> = fenced_blocks(&reference(), "yaml")
        .into_iter()
        .map(|block| block.text)
        .collect();
    for fixture in [
        "reference-config.yaml",
        "build-feature.yaml",
        "release-cycle.yaml",
    ] {
        let path = repo_root().join("crates/core/tests/fixtures").join(fixture);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            shown.contains(&text),
            "docs/reference.md shows no block that is `{}` as it stands — copy the file into its block",
            path.display()
        );
    }
}

/// Every fence of the user docs says its language: a YAML example in a
/// bare fence is one the sweep above never reads.
#[test]
fn every_fence_in_the_user_docs_says_its_language() {
    let root = repo_root();
    let mut files = vec![root.join("README.md"), root.join("SECURITY.md")];
    files.extend(
        markdown_files(&root.join("docs"))
            .into_iter()
            .filter(|file| !file.starts_with(root.join("docs/design"))),
    );
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let mut inside = false;
        for (at, line) in text.lines().enumerate() {
            if !line.trim_start().starts_with("```") {
                continue;
            }
            assert!(
                inside || line.trim() != "```",
                "{}:{}: a fence with no language — say it (`yaml`, `bash`, `text`, ...)",
                file.display(),
                at + 1
            );
            inside = !inside;
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
