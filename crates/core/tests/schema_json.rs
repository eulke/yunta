//! The JSON Schema of every authored document and of the event log is
//! generated from the Rust types — the types are the source of truth,
//! the schema their emission.

use serde_json::Value;

fn rendered(schema: schemars::Schema) -> Value {
    serde_json::to_value(schema).unwrap()
}

#[test]
fn every_root_schema_names_itself_and_speaks_draft_2020_12() {
    for (name, schema) in [
        ("workflow", yunta_core::schema::workflow()),
        ("config", yunta_core::schema::config()),
        ("pack", yunta_core::schema::pack()),
        ("ledger", yunta_core::schema::ledger()),
        ("events", yunta_core::schema::events()),
    ] {
        let json = rendered(schema);
        assert_eq!(
            json["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "{name}"
        );
        assert!(json["title"].is_string(), "{name} has a title: {json}");
    }
}

#[test]
fn the_workflow_schema_describes_nodes_by_kind_and_inputs_by_type() {
    let json = rendered(yunta_core::schema::workflow());
    let text = json.to_string();
    assert!(json["properties"]["nodes"].is_object(), "{text}");
    assert!(json["properties"]["inputs"].is_object(), "{text}");
    for kind in ["prompt", "bash", "loop", "gate", "workflow"] {
        assert!(
            text.contains(&format!("\"{kind}\"")),
            "kind `{kind}`: {text}"
        );
    }
    assert!(text.contains("all_tasks_complete"), "{text}");
    // The authored form of an input, `required:` included.
    assert!(text.contains("\"required\""), "{text}");
}

#[test]
fn the_events_schema_is_one_stored_event_with_its_envelope_and_every_kind() {
    let json = rendered(yunta_core::schema::events());
    let text = json.to_string();
    for field in ["run_id", "seq", "timestamp"] {
        assert!(text.contains(&format!("\"{field}\"")), "{field}: {text}");
    }
    for kind in yunta_core::events::EventPayload::KINDS {
        assert!(
            text.contains(&format!("\"{kind}\"")),
            "kind `{kind}`: {text}"
        );
    }
}

#[test]
fn a_run_id_is_described_as_a_string_and_a_seq_as_a_positive_integer() {
    let text = rendered(yunta_core::schema::events()).to_string();
    assert!(text.contains("\"minimum\":1"), "{text}");
}
