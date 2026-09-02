//! Every document a person writes refuses a key the schema does not
//! know, naming the key, where it sits and what is valid there.

use yunta_core::yaml;
use yunta_core::{ConfigLayer, Ledger, PackManifest, QuestionsFile, Workflow};

fn err<T: serde::de::DeserializeOwned>(text: &str) -> String {
    match yaml::parse::<T>(text) {
        Ok(_) => panic!("parsed a document with an unknown key:\n{text}"),
        Err(e) => e.to_string(),
    }
}

const NODE: &str = "name: w\nnodes:\n  - id: a\n    kind: bash\n    run: \"true\"\n";

#[test]
fn a_workflow_refuses_an_unknown_top_level_key() {
    let text = err::<Workflow>(&format!("{NODE}nodez: []\n"));
    assert!(text.contains("nodez"), "{text}");
    assert!(text.contains("`nodes`"), "the valid keys are named: {text}");
}

#[test]
fn a_node_refuses_an_unknown_key_and_names_every_one_at_once() {
    let text = err::<Workflow>(
        "name: w\nnodes:\n  - id: plan\n    kind: prompt\n    prompt: p\n    depend_on: [a]\n    scpe: []\n",
    );
    assert!(text.contains("node `plan`"), "the node is named: {text}");
    assert!(
        text.contains("`depend_on`") && text.contains("`scpe`"),
        "{text}"
    );
    assert!(
        text.contains("`depends_on`") && text.contains("`prompt`"),
        "{text}"
    );
}

#[test]
fn a_node_key_that_belongs_to_another_kind_is_refused() {
    let text =
        err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: bash\n    run: x\n    prompt: p\n");
    assert!(text.contains("`prompt`") && text.contains("bash"), "{text}");
}

#[test]
fn role_and_fresh_context_are_refused_with_the_key_that_replaces_them() {
    let text = err::<Workflow>(
        "name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    role: planner\n    fresh_context: true\n",
    );
    assert!(
        text.contains("`role`") && text.contains("`runner:`"),
        "{text}"
    );
    assert!(
        text.contains("`fresh_context`") && text.contains("`on_interrupt"),
        "{text}"
    );
}

#[test]
fn a_context_entry_names_its_source_or_is_refused() {
    let text = err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    context:\n      - filez: [x]\n");
    assert!(text.contains("filez") && text.contains("files"), "{text}");
    let text = err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    context:\n      - artifact: { node: b, name: n, nmae: x }\n");
    assert!(text.contains("nmae"), "{text}");
}

#[test]
fn an_on_finish_step_names_its_kind_or_is_refused() {
    let text = err::<Workflow>(&format!("{NODE}on_finish:\n  - clean: worktree\n"));
    assert!(
        text.contains("clean") && text.contains("cleanup") && text.contains("distill"),
        "{text}"
    );
}

#[test]
fn an_artifact_and_a_prompt_file_refuse_unknown_keys() {
    let text = err::<Workflow>(
        "name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: { file: p.md, fil: x }\n",
    );
    assert!(text.contains("fil"), "{text}");
    let text = err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    artifacts:\n      produces: [{ name: n, kind: findings, knd: x }]\n");
    assert!(text.contains("knd"), "{text}");
}

#[test]
fn a_mode_and_an_input_refuse_unknown_keys() {
    let text = err::<Workflow>(&format!(
        "{NODE}modes:\n  quick: {{ include: [a], includes: [] }}\n"
    ));
    assert!(text.contains("includes"), "{text}");
    let text = err::<Workflow>(&format!(
        "{NODE}inputs:\n  idea: {{ type: string, descripton: x }}\n"
    ));
    assert!(text.contains("descripton"), "{text}");
}

#[test]
fn a_config_layer_refuses_unknown_keys_at_every_level() {
    let text = err::<ConfigLayer>("versio: 1\n");
    assert!(
        text.contains("versio") && text.contains("`runners`"),
        "{text}"
    );
    let text = err::<ConfigLayer>("defaults:\n  on_failur: pause\n");
    assert!(
        text.contains("defaults") && text.contains("on_failur"),
        "{text}"
    );
    let text =
        err::<ConfigLayer>("runners:\n  planner:\n    - { adapter: mock, model: m, agnt: x }\n");
    assert!(text.contains("agnt"), "{text}");
    let text = err::<ConfigLayer>("permissions:\n  commands:\n    denied: [rm]\n");
    assert!(text.contains("denied") && text.contains("`deny`"), "{text}");
}

#[test]
fn a_pack_manifest_refuses_unknown_keys() {
    let text = err::<PackManifest>(
        "name: p\npublisher: acme\nversion: 0.1.0\ndeclares: { permissions: edit, netwrk: false }\n",
    );
    assert!(
        text.contains("netwrk") && text.contains("`network`"),
        "{text}"
    );
}

#[test]
fn a_ledger_refuses_unknown_keys_on_tasks_and_criteria() {
    let text = err::<Ledger>(
        "tasks:\n  - id: t\n    titel: x\n    scope: [a]\n    criteria: [{ cmd: true }]\n",
    );
    assert!(text.contains("titel") && text.contains("`title`"), "{text}");
    let text = err::<Ledger>("tasks:\n  - id: t\n    title: x\n    scope: [a]\n    criteria: [{ cmd: true, typ: guard }]\n");
    assert!(text.contains("typ") && text.contains("`type`"), "{text}");
}

#[test]
fn a_questions_artifact_refuses_unknown_keys() {
    let text = err::<QuestionsFile>("questions:\n  - id: q\n    text: t\n    answer_type: text\n    required: true\n    valeus: []\n");
    assert!(
        text.contains("valeus") && text.contains("`values`"),
        "{text}"
    );
}

#[test]
fn a_persisted_document_still_tolerates_keys_it_does_not_know() {
    // The engine writes the lock and reads it back with whatever binary
    // comes later: an unknown key there is a newer writer, never a typo.
    let lock: yunta_core::PackLock = yaml::parse("packs: {}\nsignature: abc\n").unwrap();
    assert!(lock.packs.is_empty());
}
