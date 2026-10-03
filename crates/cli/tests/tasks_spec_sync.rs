//! The tasks spec states the rules the engine holds a tasks document to:
//! the ones every document meets, and the ones a plan a person reviews
//! meets too. One list read twice, so a rule the spec leaves out fails
//! here rather than reaching a writer for the first time as a refusal.

use std::collections::BTreeSet;
use std::path::PathBuf;

use yunta_testkit::{bullets, numbered_items, rule_codes_named, section};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The rules of the tasks document are published before it is written —
/// the same list the contract hands a session — so the spec that states
/// them and the engine that enforces them are one list read twice. A rule
/// the spec leaves out is one a writer meets for the first time as a
/// failure.
#[test]
fn the_tasks_spec_states_every_rule_the_engine_publishes() {
    let spec = repo_root().join("docs/design/spec-tasks.md");
    let text = std::fs::read_to_string(&spec).unwrap();
    let items = numbered_items(&section(&text, "## 3."));
    let stated: BTreeSet<String> = items
        .iter()
        .map(|item| {
            let named = rule_codes_named(item);
            assert_eq!(
                named.len(),
                1,
                "every item of §3 names the one rule code it states, in backticks: {item}"
            );
            named[0].clone()
        })
        .collect();

    let published: BTreeSet<String> = <yunta_core::TasksFile as yunta_core::shape::Document>::RULES
        .iter()
        .map(|rule| rule.code.as_str().to_string())
        .collect();

    assert_eq!(
        stated, published,
        "§3 of the tasks spec (left) and the rules the engine publishes (right) disagree"
    );
    assert_eq!(
        items.len(),
        published.len(),
        "§3 states one item per published rule"
    );

    let review: BTreeSet<String> = bullets(&section(&text, "### 3.1"))
        .iter()
        .flat_map(|item| rule_codes_named(item))
        .collect();
    let demanded: BTreeSet<String> =
        <yunta_core::TasksFile as yunta_core::shape::Document>::REVIEW_RULES
            .iter()
            .map(|rule| rule.code.as_str().to_string())
            .collect();
    assert_eq!(
        review, demanded,
        "§3.1 of the tasks spec (left) and what a reviewed plan is held to (right) disagree"
    );
}
