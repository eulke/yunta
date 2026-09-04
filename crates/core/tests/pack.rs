//! `pack.yaml` — schema and parsing.

use yunta_core::{NodePermissions, PackLock, PackLockEntry, PackManifest, PackRef};

#[test]
fn the_reference_pack_parses_and_round_trips() {
    let yaml = include_str!("fixtures/reference-pack.yaml");
    let pack: PackManifest =
        serde_norway::from_str(yaml).expect("the reference pack manifest must parse whole");

    assert_eq!(pack.name, "review-pack");
    assert_eq!(pack.publisher, "acme");
    assert_eq!(pack.version, "1.2.0");
    assert_eq!(
        pack.description.as_deref(),
        Some("Review multi-runner con consolidación de hallazgos")
    );
    assert_eq!(pack.license.as_deref(), Some("MIT"));
    assert_eq!(pack.yunta_schema.as_deref(), Some(">=1 <2"));

    assert_eq!(pack.requires.runners.len(), 2);
    assert_eq!(pack.requires.runners[0].name, "reviewer");
    assert_eq!(
        pack.requires.runners[0].permissions,
        Some(NodePermissions::ReadOnly)
    );
    assert_eq!(pack.requires.runners[1].name, "mechanical");
    assert_eq!(pack.requires.runners[1].permissions, None);
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
    let reserialized = serde_norway::to_string(&pack).unwrap();
    let reparsed: PackManifest = serde_norway::from_str(&reserialized).unwrap();
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
    let pack: PackManifest =
        serde_norway::from_str(yaml).expect("a knowledge-only pack must parse");
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
    let pack: PackManifest = serde_norway::from_str(yaml)
        .expect("declares is the only hard requirement beyond identity");
    assert!(pack.requires.runners.is_empty());
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
    let result: Result<PackManifest, _> = serde_norway::from_str(yaml);
    assert!(
        result.is_err(),
        "a pack with no declared ceiling must fail to parse, not default to some assumed one"
    );
}

#[test]
fn yunta_lock_round_trips_and_keys_by_publisher_slash_name() {
    let mut lock = PackLock::default();
    let key: PackRef = "acme/review-pack".parse().unwrap();
    assert_eq!(key.to_string(), "acme/review-pack");
    lock.packs.insert(
        key.clone(),
        PackLockEntry {
            publisher: "acme".into(),
            name: "review-pack".into(),
            source: "https://github.com/acme/review-pack".to_string(),
            r#ref: "v1.2.0".to_string(),
            commit: "abc123def456".to_string(),
            content_hash: "deadbeef".to_string(),
        },
    );

    let yaml = serde_norway::to_string(&lock).unwrap();
    let reparsed: PackLock = serde_norway::from_str(&yaml).unwrap();
    assert_eq!(lock, reparsed);
    assert_eq!(reparsed.packs[&key].commit, "abc123def456");
}
