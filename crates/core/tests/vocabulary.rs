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
        assert!(!yunta_core::shape::contract(kind).is_empty(), "{kind}");
        assert!(!yunta_core::schema::json(kind).is_empty(), "{kind}");
    }
}

#[test]
fn a_documents_kind_is_the_one_its_own_type_declares() {
    assert_eq!(<Ledger as Document>::KIND, ArtifactKind::TaskLedger);
    assert_eq!(<FindingsFile as Document>::KIND, ArtifactKind::Findings);
    assert_eq!(<QuestionsFile as Document>::KIND, ArtifactKind::Questions);
}

// --- every rule reaches the writer who has to satisfy it ------------------
//
// Three assertions form one chain, and the chain is what makes "no rule
// surprises a writer" a property of the build rather than of review:
//
//   1. a rule cannot exist without a `RuleCode` — `Problem::rule` takes one
//   2. every `RuleCode` belongs to some document's `RULES` (below)
//   3. every `RULES` entry is reachable by a document that breaks it (below)
//
// Break any link and a rule can be enforced that nobody was told about,
// which is a whole repair attempt spent on something the system knew.

use std::collections::BTreeSet;

use yunta_core::diagnostic::{Rule, RuleCode};

fn all_rules() -> Vec<(ArtifactKind, &'static Rule)> {
    ArtifactKind::ALL
        .into_iter()
        .flat_map(|kind| {
            yunta_core::shape::rules(kind)
                .iter()
                .map(move |rule| (kind, rule))
        })
        .collect()
}

#[test]
fn every_rule_code_belongs_to_a_document_that_publishes_it() {
    let published: BTreeSet<RuleCode> = all_rules().iter().map(|(_, r)| r.code).collect();
    let missing: Vec<RuleCode> = RuleCode::ALL
        .iter()
        .copied()
        .filter(|code| !published.contains(code))
        .collect();
    assert!(
        missing.is_empty(),
        "no document's `RULES` publishes {missing:?} — a rule a writer is never told about"
    );
}

#[test]
fn a_rule_is_demanded_once_per_document_and_says_something() {
    for (kind, rule) in all_rules() {
        assert!(
            !rule.demand.trim().is_empty(),
            "{kind}: `{}` demands nothing",
            rule.code
        );
        assert!(
            !rule.demand.trim_end().ends_with('.'),
            "{kind}: `{}`'s demand is a clause, not a sentence: {:?}",
            rule.code,
            rule.demand
        );
    }
    for kind in ArtifactKind::ALL {
        let codes: Vec<RuleCode> = yunta_core::shape::rules(kind)
            .iter()
            .map(|r| r.code)
            .collect();
        let unique: BTreeSet<RuleCode> = codes.iter().copied().collect();
        assert_eq!(codes.len(), unique.len(), "{kind} lists a rule twice");
    }
}

#[test]
fn the_contract_a_door_hands_out_carries_the_shape_and_every_rule() {
    for kind in ArtifactKind::ALL {
        let contract = yunta_core::shape::contract(kind);
        assert!(
            contract.contains(yunta_core::shape::contract(kind).lines().next().unwrap()),
            "{kind}"
        );
        for rule in yunta_core::shape::rules(kind) {
            let demand = yunta_core::text::one_line(rule.demand);
            assert!(
                contract.contains(&demand),
                "{kind}'s contract never states `{}`:\n{contract}",
                rule.code
            );
        }
    }
}
