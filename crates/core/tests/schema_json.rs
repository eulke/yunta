//! The JSON Schema of every authored document and of the event log is
//! generated from the Rust types — the types are the source of truth,
//! the schema their emission.

use serde_json::Value;
use yunta_core::events::EventPayload;

fn rendered(schema: schemars::Schema) -> Value {
    serde_json::to_value(schema).unwrap()
}

/// The `kind` discriminant pinned by every branch of a `oneOf`, in
/// declaration order.
fn one_of_kinds(schema: &Value) -> Vec<&str> {
    schema["oneOf"]
        .as_array()
        .expect("a oneOf schema")
        .iter()
        .map(|branch| {
            branch["properties"]["kind"]["const"]
                .as_str()
                .expect("each branch pins its kind")
        })
        .collect()
}

#[test]
fn every_root_schema_names_itself_and_speaks_draft_2020_12() {
    for (title, schema) in [
        ("yunta workflow", yunta_core::schema::workflow()),
        ("yunta config layer", yunta_core::schema::config()),
        ("yunta pack manifest", yunta_core::schema::pack()),
        ("yunta tasks", yunta_core::schema::tasks()),
        ("yunta event", yunta_core::schema::events()),
    ] {
        let json = rendered(schema);
        assert_eq!(
            json["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "{title}"
        );
        assert_eq!(json["title"], title);
    }
}

#[test]
fn the_workflow_schema_describes_nodes_by_kind_and_inputs_by_type() {
    let json = rendered(yunta_core::schema::workflow());
    // `nodes:` is a list of nodes; `inputs:` a map to the authored input form.
    assert_eq!(json["properties"]["nodes"]["items"]["$ref"], "#/$defs/Node");
    assert_eq!(
        json["properties"]["inputs"]["additionalProperties"]["$ref"],
        "#/$defs/InputSpec"
    );
    // A node is described by kind: one branch per kind, each pinned by
    // its `kind` discriminant.
    assert_eq!(
        one_of_kinds(&json["$defs"]["Node"]),
        [
            "prompt",
            "bash",
            "loop",
            "parallel",
            "check",
            "executor",
            "gate",
            "workflow",
            "pull_request"
        ]
    );
    // The one loop-until condition, as an exhaustive enum.
    assert_eq!(
        json["$defs"]["LoopUntil"]["enum"],
        serde_json::json!(["all_tasks_complete"])
    );
    // Every authored input form (one branch per type) declares `required:`.
    let forms = json["$defs"]["InputSpec"]["oneOf"]
        .as_array()
        .expect("InputSpec is a oneOf over its types");
    assert!(
        forms
            .iter()
            .all(|form| form["properties"]["required"]["$ref"] == "#/$defs/Requiredness"),
        "every authored input form declares `required:`: {forms:?}"
    );
}

#[test]
fn the_events_schema_is_one_stored_event_with_its_envelope_and_every_kind() {
    let json = rendered(yunta_core::schema::events());
    // The envelope every stored event carries, side by side with its payload.
    assert_eq!(
        json["$defs"]["Envelope"]["required"],
        serde_json::json!(["run_id", "seq", "timestamp"])
    );
    // One payload branch per kind, in the order the binary lists them.
    assert_eq!(
        one_of_kinds(&json["$defs"]["EventPayload"]),
        EventPayload::KINDS.to_vec()
    );
}

#[test]
fn a_gate_resolution_is_one_flat_object_of_four_optional_fields() {
    // The wire shape is what the schema publishes: not the shapes the
    // engine reads out of it.
    let json = rendered(yunta_core::schema::events());
    let resolved = &json["$defs"]["GateResolvedPayload"];
    assert_eq!(resolved["type"], "object");
    let mut fields: Vec<&str> = resolved["properties"]
        .as_object()
        .expect("an object with properties")
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        ["approved_sha", "chosen_option", "free_text", "resolved_by"]
    );
    assert!(
        resolved.get("required").is_none(),
        "every field is optional"
    );
    assert!(
        resolved.get("oneOf").is_none(),
        "no variant branches on the wire"
    );
}

#[test]
fn a_run_id_is_described_as_a_string_and_a_seq_as_a_positive_integer() {
    let json = rendered(yunta_core::schema::events());
    assert_eq!(json["$defs"]["RunId"]["type"], "string");
    assert_eq!(json["$defs"]["Seq"]["type"], "integer");
    assert_eq!(json["$defs"]["Seq"]["minimum"], 1);
}

/// Every `description` in `value`, with the path that carries it.
fn descriptions<'a>(value: &'a Value, at: &str, found: &mut Vec<(String, &'a str)>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                match (key.as_str(), child) {
                    ("description", Value::String(text)) => found.push((at.to_string(), text)),
                    _ => descriptions(child, &format!("{at}/{key}"), found),
                }
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                descriptions(child, &format!("{at}/{index}"), found);
            }
        }
        _ => {}
    }
}

#[test]
fn a_published_document_says_nothing_of_the_code_that_reads_it() {
    // What a writer of the document reads: a session in its tool's
    // arguments, a person through `yunta schema`. The words that only a
    // maintainer of the parser has use for stay in the code.
    let code_words = ["[`", "serde", "event log", "mirroring", "verbatim"];
    for (name, schema) in [
        ("tasks", yunta_core::schema::tasks()),
        ("spec", yunta_core::schema::spec()),
        ("findings", yunta_core::schema::findings()),
        ("questions", yunta_core::schema::questions()),
        ("answers", yunta_core::schema::answers()),
        ("withdrawal", yunta_core::schema::withdrawal()),
    ] {
        let json = rendered(schema);
        let mut found = Vec::new();
        descriptions(&json, name, &mut found);
        for (at, text) in found {
            for word in code_words {
                assert!(
                    !text.contains(word),
                    "{at} speaks of the code with `{word}`: {text}"
                );
            }
        }
    }
}
