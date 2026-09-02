//! The authored schema has no tri-state and no accidental order: a flag
//! is a bool with a declared default, a closed choice is an exhaustive
//! enum, an input's optionality follows from its default, and every map
//! serializes in key order.

use std::collections::BTreeMap;

use yunta_core::events::TokenUsage;
use yunta_core::{
    yaml, AdapterId, AdapterSettings, ConfigLayer, LoopUntil, McpServerConfig, NodeKind,
    NodePermissions, PricingEntry, RunnerCandidate, RunnerName, ScopeExpansionMode, Workflow,
};

fn workflow(text: &str) -> Workflow {
    yaml::parse(text).unwrap()
}

fn refused<T: serde::de::DeserializeOwned>(text: &str) -> String {
    match yaml::parse::<T>(text) {
        Ok(_) => panic!("parsed a document that contradicts itself:\n{text}"),
        Err(e) => e.to_string(),
    }
}

const BASH: &str = "nodes:\n  - id: a\n    kind: bash\n    run: \"true\"\n";

#[test]
fn loop_until_is_an_exhaustive_enum() {
    let wf = workflow(
        "name: w\nnodes:\n  - id: l\n    kind: loop\n    until: all_tasks_complete\n    prompt: p\n",
    );
    assert!(matches!(
        wf.nodes[0].kind,
        NodeKind::Loop {
            until: LoopUntil::AllTasksComplete,
            ..
        }
    ));
    assert_eq!(LoopUntil::AllTasksComplete.as_str(), "all_tasks_complete");

    let text = refused::<Workflow>(
        "name: w\nnodes:\n  - id: l\n    kind: loop\n    until: forever\n    prompt: p\n",
    );
    assert!(
        text.contains("forever") && text.contains("all_tasks_complete"),
        "{text}"
    );
}

#[test]
fn node_network_and_interactive_are_false_unless_declared() {
    let wf = workflow(
        "name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n  - id: b\n    kind: prompt\n    prompt: p\n    network: true\n    interactive: true\n",
    );
    assert!(!wf.nodes[0].network && !wf.nodes[0].interactive);
    assert!(wf.nodes[1].network && wf.nodes[1].interactive);
}

#[test]
fn an_input_is_required_exactly_when_it_has_no_default() {
    let wf = workflow(&format!(
        "name: w\ninputs:\n  idea: {{ type: string, required: true }}\n  tone: {{ type: string, default: calm }}\n  count: {{ type: number }}\n{BASH}"
    ));
    assert!(wf.inputs["idea"].is_required());
    assert!(!wf.inputs["tone"].is_required());
    assert!(wf.inputs["count"].is_required());
    // Derived, so never stored: the frozen form carries the default only.
    let frozen = yaml::to_string(&wf.inputs).unwrap();
    assert!(!frozen.contains("required"), "{frozen}");
}

#[test]
fn an_input_that_contradicts_itself_is_refused_at_parse() {
    let text = refused::<Workflow>(&format!(
        "name: w\ninputs:\n  idea: {{ type: string, required: true, default: x }}\n{BASH}"
    ));
    assert!(text.contains("idea") && text.contains("default"), "{text}");
    let text = refused::<Workflow>(&format!(
        "name: w\ninputs:\n  idea: {{ type: string, required: false }}\n{BASH}"
    ));
    assert!(text.contains("idea") && text.contains("default"), "{text}");
}

#[test]
fn config_maps_are_ordered_by_key() {
    let config: ConfigLayer = yaml::parse(
        "runners:\n  zeta: [{ adapter: mock, model: m }]\n  alpha: [{ adapter: mock, model: m }]\n",
    )
    .unwrap();
    let runners: &BTreeMap<RunnerName, Vec<RunnerCandidate>> = config.runners.as_ref().unwrap();
    assert_eq!(
        runners.keys().map(RunnerName::as_str).collect::<Vec<_>>(),
        ["alpha", "zeta"]
    );
    let text = yaml::to_string(&config).unwrap();
    assert!(text.find("alpha").unwrap() < text.find("zeta").unwrap());
    let _: Option<&BTreeMap<AdapterId, AdapterSettings>> = config.adapters.as_ref();
    let _: Option<&BTreeMap<String, McpServerConfig>> = config.mcp_servers.as_ref();
    let _: Option<&BTreeMap<String, PricingEntry>> = config.pricing.as_ref();
}

#[test]
fn token_usage_adds_field_by_field_and_keeps_cached_unknown_when_nobody_reported_it() {
    let a = TokenUsage {
        input: 1,
        output: 2,
        cached: None,
    };
    let b = TokenUsage {
        input: 10,
        output: 20,
        cached: Some(5),
    };
    assert_eq!(
        a + a,
        TokenUsage {
            input: 2,
            output: 4,
            cached: None
        }
    );
    assert_eq!(
        a + b,
        TokenUsage {
            input: 11,
            output: 22,
            cached: Some(5)
        }
    );
    let mut total = TokenUsage::default();
    total += b;
    total += b;
    assert_eq!(total.cached, Some(10));
    assert_eq!([a, b].into_iter().sum::<TokenUsage>(), a + b);
}

#[test]
fn node_permissions_name_their_yaml_spelling() {
    assert_eq!(NodePermissions::ReadOnly.as_str(), "read-only");
    assert_eq!(NodePermissions::Edit.as_str(), "edit");
    assert_eq!(NodePermissions::Full.as_str(), "full");
}

#[test]
fn scope_expansion_mode_is_a_policy() {
    assert_eq!(
        yunta_core::policy::ScopeExpansionMode::default(),
        ScopeExpansionMode::Deny
    );
}
