//! `pack.yaml` (RFC-0002 §3, T11.1) — schema and parsing.

use yunta_core::{NodePermissions, PackManifest};

#[test]
fn the_reference_pack_parses_and_round_trips() {
    let yaml = include_str!("fixtures/reference-pack.yaml");
    let pack: PackManifest =
        serde_yaml::from_str(yaml).expect("the reference pack manifest must parse whole");

    assert_eq!(pack.name, "review-pack");
    assert_eq!(pack.publisher, "acme");
    assert_eq!(pack.version, "1.2.0");
    assert_eq!(
        pack.description.as_deref(),
        Some("Review multi-runner con consolidación de hallazgos")
    );
    assert_eq!(pack.license.as_deref(), Some("MIT"));
    assert_eq!(pack.yunta_schema.as_deref(), Some(">=1 <2"));

    assert_eq!(pack.requires.roles.len(), 2);
    assert_eq!(pack.requires.roles[0].name, "reviewer");
    assert_eq!(
        pack.requires.roles[0].permissions,
        Some(NodePermissions::ReadOnly)
    );
    assert_eq!(pack.requires.roles[1].name, "mechanical");
    assert_eq!(pack.requires.roles[1].permissions, None);
    assert_eq!(pack.requires.mcp_servers, vec!["internal-docs"]);
    assert_eq!(pack.requires.commands, vec!["gh", "cargo"]);

    assert_eq!(pack.declares.permissions, NodePermissions::ReadOnly);
    assert!(!pack.declares.network);
    assert!(pack.declares.executors.is_empty());

    assert_eq!(
        pack.contents.workflows,
        vec!["review.yaml", "review-docs.yaml"]
    );
    assert_eq!(pack.contents.skills, vec!["review-rubric/"]);
    assert!(pack.contents.knowledge.is_empty());
    assert_eq!(pack.contents.docs, vec!["README.md"]);

    // Round-trip at the serde-tree level: what parses serializes back to
    // the same value.
    let reserialized = serde_yaml::to_string(&pack).unwrap();
    let reparsed: PackManifest = serde_yaml::from_str(&reserialized).unwrap();
    assert_eq!(pack, reparsed);
}

#[test]
fn a_pack_may_be_knowledge_only_with_no_workflows_or_skills() {
    let yaml = r#"
name: org-knowledge
publisher: acme
version: 1.0.0
declares:
  permissions: read-only
contents:
  knowledge: [adrs/, conventions.md]
"#;
    let pack: PackManifest = serde_yaml::from_str(yaml).expect("a knowledge-only pack must parse");
    assert!(pack.contents.workflows.is_empty());
    assert!(pack.contents.skills.is_empty());
    assert_eq!(pack.contents.knowledge, vec!["adrs/", "conventions.md"]);
}

#[test]
fn requires_and_contents_default_to_empty_when_omitted() {
    let yaml = r#"
name: minimal
publisher: acme
version: 0.1.0
declares:
  permissions: full
"#;
    let pack: PackManifest =
        serde_yaml::from_str(yaml).expect("declares is the only hard requirement beyond identity");
    assert!(pack.requires.roles.is_empty());
    assert!(pack.requires.mcp_servers.is_empty());
    assert!(pack.requires.commands.is_empty());
    assert!(pack.contents.workflows.is_empty());
}

#[test]
fn declares_is_mandatory_the_ceiling_is_never_implicit() {
    let yaml = r#"
name: no-ceiling
publisher: acme
version: 0.1.0
"#;
    let result: Result<PackManifest, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "a pack with no declared ceiling must fail to parse, not default to some assumed one"
    );
}
