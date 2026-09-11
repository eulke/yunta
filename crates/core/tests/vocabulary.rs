//! Every closed set this crate names twice — once for serde, once for
//! the sentence a diagnostic writes — says the same thing both times.
//!
//! Each of these lists exists because generating it at runtime would put
//! schema generation in the shipped binary. That makes the restatement a
//! deliberate cost, and these tests are what it buys: a list that cannot
//! drift from the parser without failing here, before the merge.

use yunta_core::events::FindingSeverity;
use yunta_core::shape::Document;
use yunta_core::{AnswerType, ArtifactKind, FindingsFile, Ledger, QuestionsFile};

/// What serde writes for a value, unquoted.
fn serialized(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .expect("a closed set serializes")
        .as_str()
        .expect("as a string")
        .to_string()
}

#[test]
fn a_kind_spells_itself_the_way_serde_spells_it() {
    for kind in ArtifactKind::ALL {
        assert_eq!(serialized(&kind), kind.as_str(), "{kind:?}");
    }
}

#[test]
fn a_kind_reads_back_from_the_name_it_publishes() {
    for kind in ArtifactKind::ALL {
        assert_eq!(
            kind.as_str().parse::<ArtifactKind>().expect("round-trips"),
            kind
        );
    }
}

#[test]
fn a_kind_that_does_not_exist_names_the_ones_that_do() {
    let error = "plan".parse::<ArtifactKind>().expect_err("no such kind");
    let text = error.to_string();
    assert!(text.contains("`plan`"), "{text}");
    for kind in ArtifactKind::ALL {
        assert!(text.contains(kind.as_str()), "{text}");
    }
}

#[test]
fn the_severity_ladder_a_diagnostic_lists_is_the_one_the_parser_accepts() {
    let parsed: Vec<String> = FindingSeverity::NAMES
        .iter()
        .map(|name| {
            serialized(
                &yunta_core::yaml::parse::<FindingSeverity>(name)
                    .unwrap_or_else(|_| panic!("`{name}` is on the ladder")),
            )
        })
        .collect();
    assert_eq!(parsed, FindingSeverity::NAMES.to_vec());
}

#[test]
fn the_answer_types_a_diagnostic_lists_are_the_ones_the_parser_accepts() {
    let parsed: Vec<String> = AnswerType::NAMES
        .iter()
        .map(|name| {
            serialized(
                &yunta_core::yaml::parse::<AnswerType>(name)
                    .unwrap_or_else(|_| panic!("`{name}` is an answer type")),
            )
        })
        .collect();
    assert_eq!(parsed, AnswerType::NAMES.to_vec());
}

// --- the published schema is the committed one -------------------------

#[test]
fn the_schema_a_door_publishes_is_the_one_the_repository_checked() {
    for (kind, schema) in [
        (ArtifactKind::TaskLedger, yunta_core::schema::ledger()),
        (ArtifactKind::Findings, yunta_core::schema::findings()),
        (ArtifactKind::Questions, yunta_core::schema::questions()),
    ] {
        let embedded: serde_json::Value = serde_json::from_str(yunta_core::schema::json(kind))
            .unwrap_or_else(|e| panic!("{kind} embeds valid JSON: {e}"));
        let generated = serde_json::to_value(schema).expect("a schema renders");
        assert_eq!(embedded, generated, "{kind}");
    }
}

#[test]
fn every_kind_publishes_a_shape_and_a_schema() {
    for kind in ArtifactKind::ALL {
        assert!(!yunta_core::shape::published(kind).is_empty(), "{kind}");
        assert!(!yunta_core::schema::json(kind).is_empty(), "{kind}");
    }
}

#[test]
fn a_documents_kind_is_the_one_its_own_type_declares() {
    assert_eq!(<Ledger as Document>::KIND, ArtifactKind::TaskLedger);
    assert_eq!(<FindingsFile as Document>::KIND, ArtifactKind::Findings);
    assert_eq!(<QuestionsFile as Document>::KIND, ArtifactKind::Questions);
}
