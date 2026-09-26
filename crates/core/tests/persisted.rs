//! What core's own persisted documents promise, and what a tolerant
//! reader does with a file a newer writer wrote.

use std::collections::BTreeMap;

use yunta_core::persisted::{Encoding, Persisted, PersistedDoc, PersistedError};
use yunta_core::PackLock;
use yunta_testkit_core::persisted::holds_its_version;

#[test]
fn every_persisted_file_carries_its_version() {
    holds_its_version(PackLock::default());
    holds_its_version(manifest());
}

/// The smallest manifest a run could be created with — enough to
/// assert what every persisted document owes, which is about the
/// document rather than about any one run.
fn manifest() -> yunta_core::Manifest {
    yunta_core::Manifest {
        schema_version: <yunta_core::Manifest as Persisted>::SCHEMA_VERSION,
        yunta_version: "0.0.5".to_string(),
        workflow: yunta_core::yaml::parse(
            "name: ship\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n",
        )
        .expect("a workflow"),
        config: yunta_core::ConfigLayer::default(),
        inputs: BTreeMap::new(),
        prompts: BTreeMap::new(),
        base_branch: "main".to_string(),
        base_commit: "deadbeef".into(),
        isolation: yunta_core::Isolation::None,
        max_parallel_nodes: 1,
        workflow_hash: yunta_core::sha256_hex(b"workflow"),
        config_hash: yunta_core::sha256_hex(b"config"),
        paths: None,
        pack: None,
    }
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct JsonDocument {
    name: String,
}

impl Persisted for JsonDocument {
    const SCHEMA_VERSION: u32 = 1;
    const NAME: &'static str = "JSON document";
    const ENCODING: Encoding = Encoding::Json;
}

#[test]
fn a_json_persisted_document_keeps_its_encoding_and_unknown_fields() {
    let read =
        PersistedDoc::<JsonDocument>::read(br#"{"schema_version":1,"name":"kept","future":42}"#)
            .expect("JSON document reads");
    assert_eq!(read.doc.name, "kept");
    assert_eq!(read.unknown_keys(), ["future"]);
    let bytes = read.write().expect("JSON document writes");
    let back: serde_json::Value = serde_json::from_slice(&bytes).expect("still JSON");
    assert_eq!(back["name"], "kept");
    assert_eq!(back["future"], 42);
    assert_eq!(back["schema_version"], 1);
}

#[test]
fn what_a_newer_writer_added_survives_an_older_binary_rewriting_the_file() {
    // The whole reason a persisted document is tolerant: an older
    // binary that read a newer file and wrote it back must not silently
    // delete what the newer one recorded.
    let written = "schema_version: 1\npacks: {}\nsomething_later: [1, 2]\n";
    let read =
        PersistedDoc::<PackLock>::read(written.as_bytes()).expect("an older binary reads it");
    assert_eq!(read.unknown_keys(), ["something_later"]);
    assert_eq!(read.doc, PackLock::default());

    let back = String::from_utf8(read.write().expect("and writes it back")).expect("as text");
    assert!(
        back.contains("something_later"),
        "what it did not understand is still there: {back}"
    );
}

#[test]
fn a_file_from_a_newer_writer_says_which_version_this_binary_supports() {
    let ahead = format!(
        "schema_version: {}\npacks: {{}}\n",
        <PackLock as Persisted>::SCHEMA_VERSION + 1
    );
    let error = PersistedDoc::<PackLock>::read(ahead.as_bytes()).expect_err("too new to read");
    assert!(matches!(error, PersistedError::NewerWriter { .. }));
    let text = error.to_string();
    assert!(text.contains("pack lock"), "{text}");
    assert!(
        text.contains("upgrade yunta"),
        "what to do about it: {text}"
    );
}

#[test]
fn a_file_stamped_with_no_version_is_read_as_the_oldest_one() {
    // Every file written before its type stamped a version: older than
    // this binary, so this binary reads it — which is the whole of what
    // tolerance means in this direction.
    let read = PersistedDoc::<PackLock>::read(b"packs: {}\n").expect("an unstamped lock reads");
    assert_eq!(read.schema_version, 0);
    assert_eq!(read.unknown, BTreeMap::new());
}

#[test]
fn a_file_that_is_not_the_document_is_refused_as_that_document() {
    let error = PersistedDoc::<PackLock>::read(b"packs: \"not a map\"\n")
        .expect_err("that is not a pack lock");
    assert!(matches!(error, PersistedError::Unreadable { .. }));
    assert!(error.to_string().contains("pack lock"));
}

/// A manifest frozen while `inherit` was still a word reads as what that
/// word meant, at the run's own level and at a node's alike: refusing it
/// would strand a run that is mid-flight, and a frozen file is never
/// rewritten to be readable.
#[test]
fn a_frozen_manifest_that_says_inherit_reads_as_none() {
    let frozen = format!(
        "schema_version: {}\n\
         yunta_version: \"0.0.5\"\n\
         isolation: inherit\n\
         base_branch: main\n\
         base_commit: deadbeef\n\
         max_parallel_nodes: 1\n\
         workflow_hash: {}\n\
         config_hash: {}\n\
         inputs: {{}}\n\
         prompts: {{}}\n\
         config: {{}}\n\
         workflow:\n\
         \x20 name: ship\n\
         \x20 nodes:\n\
         \x20   - id: phase-1\n\
         \x20     kind: workflow\n\
         \x20     use: implement-phase\n\
         \x20     isolation: inherit\n",
        <yunta_core::Manifest as Persisted>::SCHEMA_VERSION,
        yunta_core::sha256_hex(b"workflow"),
        yunta_core::sha256_hex(b"config"),
    );

    let read: PersistedDoc<yunta_core::Manifest> =
        PersistedDoc::read(frozen.as_bytes()).expect("a manifest this binary can still act on");

    assert_eq!(read.doc.isolation, yunta_core::Isolation::None);
    let yunta_core::NodeKind::Workflow { isolation, .. } = &read.doc.workflow.nodes[0].kind else {
        panic!("expected a workflow node");
    };
    assert_eq!(*isolation, yunta_core::Isolation::None);
}

#[test]
fn a_frozen_fanout_manifest_round_trips_without_reordering_modes() {
    let mut manifest = manifest();
    manifest.workflow = yunta_core::workflow::read::read(
        "name: ship\nmodes:\n  quick: { include: [review] }\n  standard: { include: [review] }\n  full: { include: all }\nnodes:\n  - id: review\n    kind: prompt\n    runners: [first, second]\n    prompt: audit\n",
        std::path::Path::new("ship.yaml"),
    ).expect("an authored fan-out expands");
    let bytes = PersistedDoc::of(manifest.clone())
        .write()
        .expect("manifest writes");
    let text = String::from_utf8(bytes.clone()).expect("YAML is UTF-8");
    let quick = text.find("quick:").expect("quick is present");
    let standard = text.find("standard:").expect("standard is present");
    let full = text.find("full:").expect("full is present");
    assert!(
        quick < standard && standard < full,
        "declaration order survives writing: {text}"
    );
    let read = PersistedDoc::<yunta_core::Manifest>::read(&bytes).expect("generated ids read");
    let modes: Vec<_> = read
        .doc
        .workflow
        .modes
        .as_ref()
        .expect("modes")
        .keys()
        .map(|name| name.as_str())
        .collect();
    assert_eq!(modes, ["quick", "standard", "full"]);
    let ids: Vec<_> = read
        .doc
        .workflow
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect();
    assert_eq!(ids, ["review@first", "review@second"]);
    assert_eq!(read.doc.workflow, manifest.workflow);
}
