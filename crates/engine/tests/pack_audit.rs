//! `yunta pack audit`: `audit_pack`'s
//! completeness against a fixture pack's actual content, and the
//! "prompts never trimmed" guarantee — pure filesystem fixtures, no
//! live run needed.

use std::path::Path;

use yunta_core::PackManifest;
use yunta_engine::audit_pack;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn manifest(pack_dir: &Path) -> PackManifest {
    let text = std::fs::read_to_string(pack_dir.join("pack.yaml")).unwrap();
    serde_yaml::from_str(&text).unwrap()
}

/// A pack exercising every audit-worthy surface in one workflow: a
/// `bash` command, a `loop` with its own `until` criterion, hooks
/// before/after, an inline prompt and a `{file: ...}` prompt, every
/// `context:` source kind (including one naming an `mcp` server), node
/// permissions, a node-level `agent:`, and an `executor` node.
fn write_full_pack(root: &Path) {
    write(
        &root.join("pack.yaml"),
        "name: kitchen-sink\npublisher: acme\nversion: 1.0.0\n\
         declares:\n  permissions: edit\n  network: true\n  executors: [runner.py]\n\
         contents:\n  workflows: [review.yaml]\n",
    );
    write(
        &root.join("review.yaml"),
        "name: review\n\
         nodes:\n\
         \x20 - id: lint\n\
         \x20   kind: bash\n\
         \x20   run: \"cargo clippy\"\n\
         \x20 - id: fix\n\
         \x20   kind: loop\n\
         \x20   until: \"cargo test\"\n\
         \x20   prompt: \"fix it, {{run.dir}}\"\n\
         \x20   hooks:\n\
         \x20     before: [{ run: \"echo before\" }]\n\
         \x20     after: [{ run: \"echo after\" }]\n\
         \x20 - id: draft\n\
         \x20   kind: prompt\n\
         \x20   prompt: { file: prompts/draft.md }\n\
         \x20   permissions: read-only\n\
         \x20   agent: reviewer-agent\n\
         \x20   context:\n\
         \x20     - files: [src/lib.rs]\n\
         \x20     - command: \"git log -1\"\n\
         \x20     - mcp: { server: internal-docs, query: \"{{inputs.idea}}\" }\n\
         \x20 - id: package\n\
         \x20   kind: executor\n\
         \x20   executor: runner.py\n",
    );
    write(
        &root.join("prompts/draft.md"),
        "This is the full draft prompt.\nIt spans several lines.\nNothing here is a summary.\n",
    );
}

#[test]
fn the_inventory_is_exhaustive_against_the_packs_own_content() {
    let root = tempfile::tempdir().unwrap();
    write_full_pack(root.path());

    let audit = audit_pack(root.path(), manifest(root.path()));

    assert_eq!(audit.workflows.len(), 1);
    let workflow = &audit.workflows[0];
    assert!(workflow.error.is_none(), "{:?}", workflow.error);
    assert_eq!(workflow.nodes.len(), 4);

    let lint = &workflow.nodes[0];
    assert_eq!(lint.kind, "bash");
    assert_eq!(lint.command.as_deref(), Some("cargo clippy"));

    let fix = &workflow.nodes[1];
    assert_eq!(fix.kind, "loop");
    assert_eq!(fix.command.as_deref(), Some("cargo test"));
    assert_eq!(fix.hooks_before, vec!["echo before".to_string()]);
    assert_eq!(fix.hooks_after, vec!["echo after".to_string()]);
    match fix.prompt.as_ref().unwrap() {
        Ok(text) => assert_eq!(text, "fix it, {{run.dir}}"),
        Err(e) => panic!("expected an inline prompt, got an error: {e}"),
    }

    let draft = &workflow.nodes[2];
    assert_eq!(draft.kind, "prompt");
    assert_eq!(draft.permissions, Some("read-only"));
    assert_eq!(draft.agent.as_deref(), Some("reviewer-agent"));
    assert_eq!(draft.context.len(), 3);
    assert!(draft.context[0].contains("src/lib.rs"));
    assert!(draft.context[1].contains("git log -1"));
    assert!(draft.context[2].contains("internal-docs"));
    assert_eq!(draft.mcp_servers, vec!["internal-docs".to_string()]);

    let package = &workflow.nodes[3];
    assert_eq!(package.kind, "executor");
    assert_eq!(package.executor.as_deref(), Some("runner.py"));

    // Manifest-level declares/requires stay visible on the report too —
    // nothing about the audit is workflow-only.
    assert_eq!(
        audit.manifest.declares.executors,
        vec!["runner.py".to_string()]
    );
}

#[test]
fn a_file_prompt_is_resolved_to_its_full_untrimmed_text() {
    let root = tempfile::tempdir().unwrap();
    write_full_pack(root.path());

    let audit = audit_pack(root.path(), manifest(root.path()));
    let draft = &audit.workflows[0].nodes[2];
    let prompt = draft.prompt.as_ref().expect("draft is a prompt node");
    let text = prompt.as_ref().expect("prompts/draft.md exists");

    assert_eq!(
        text,
        "This is the full draft prompt.\nIt spans several lines.\nNothing here is a summary.\n"
    );
}

#[test]
fn a_missing_file_prompt_is_reported_not_silently_dropped() {
    let root = tempfile::tempdir().unwrap();
    write(
        &root.path().join("pack.yaml"),
        "name: broken\npublisher: acme\nversion: 1.0.0\n\
         declares:\n  permissions: read-only\n\
         contents:\n  workflows: [review.yaml]\n",
    );
    write(
        &root.path().join("review.yaml"),
        "name: review\nnodes:\n  - id: draft\n    kind: prompt\n    prompt: { file: missing.md }\n",
    );

    let audit = audit_pack(root.path(), manifest(root.path()));
    let draft = &audit.workflows[0].nodes[0];
    let prompt = draft.prompt.as_ref().unwrap();
    assert!(prompt.is_err(), "{prompt:?}");
}

#[test]
fn a_workflow_the_manifest_declares_but_doesnt_ship_is_still_in_the_report() {
    let root = tempfile::tempdir().unwrap();
    write(
        &root.path().join("pack.yaml"),
        "name: ghost\npublisher: acme\nversion: 1.0.0\n\
         declares:\n  permissions: read-only\n\
         contents:\n  workflows: [nowhere.yaml]\n",
    );

    let audit = audit_pack(root.path(), manifest(root.path()));
    assert_eq!(audit.workflows.len(), 1);
    assert_eq!(audit.workflows[0].declared_path, "nowhere.yaml");
    assert!(audit.workflows[0].error.is_some());
}
