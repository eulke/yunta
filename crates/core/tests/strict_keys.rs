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
    assert_eq!(
        text,
        "`nodez`: unknown field `nodez`, expected one of `name`, `description`, `modes`, `inputs`, `node_defaults`, `nodes`, `yunta_schema`, `on_finish` at line 6 column 1"
    );
}

#[test]
fn a_node_refuses_an_unknown_key_and_names_every_one_at_once() {
    let text = err::<Workflow>(
        "name: w\nnodes:\n  - id: plan\n    kind: prompt\n    prompt: p\n    depend_on: [a]\n    scpe: []\n",
    );
    assert_eq!(
        text,
        "`nodes[0]`: nodes: node `plan`: unknown key(s) `depend_on`, `scpe` for a `prompt` node; valid keys: `id`, `depends_on`, `scope`, `runner`, `runners`, `agent`, `artifacts`, `hooks`, `on_failure`, `on_interrupt`, `description`, `permissions`, `network`, `context`, `skills`, `interactive`, `invariant`, `kind`, `prompt` at line 3 column 3"
    );
}

#[test]
fn a_node_key_that_belongs_to_another_kind_is_refused() {
    let text =
        err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: bash\n    run: x\n    prompt: p\n");
    assert_eq!(
        text,
        "`nodes[0]`: nodes: node `a`: unknown key(s) `prompt` for a `bash` node; valid keys: `id`, `depends_on`, `scope`, `runner`, `runners`, `agent`, `artifacts`, `hooks`, `on_failure`, `on_interrupt`, `description`, `permissions`, `network`, `context`, `skills`, `interactive`, `invariant`, `kind`, `run` at line 3 column 3"
    );
}

#[test]
fn role_and_fresh_context_are_refused_with_the_key_that_replaces_them() {
    let text = err::<Workflow>(
        "name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    role: planner\n    fresh_context: true\n",
    );
    assert_eq!(
        text,
        "`nodes[0]`: nodes: node `a`: unknown key(s) `role`, `fresh_context` for a `prompt` node; valid keys: `id`, `depends_on`, `scope`, `runner`, `runners`, `agent`, `artifacts`, `hooks`, `on_failure`, `on_interrupt`, `description`, `permissions`, `network`, `context`, `skills`, `interactive`, `invariant`, `kind`, `prompt`; `role`: a node names its runner with `runner:`; `fresh_context`: every session starts fresh; `on_interrupt: resume_session` reuses one only when a run resumes at line 3 column 3"
    );
}

#[test]
fn a_context_entry_names_its_source_or_is_refused() {
    let text = err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    context:\n      - filez: [x]\n");
    assert_eq!(
        text,
        "`nodes[0]`: nodes: node `a`: `context[0]`: unknown key `filez` for a context source; one of `files`, `command`, `artifact`, `mcp`, `run-events`, `ledger`, `knowledge`, `node-output` at line 3 column 3"
    );
    let text = err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    context:\n      - artifact: { node: b, name: n, nmae: x }\n");
    assert_eq!(
        text,
        "`nodes[0]`: nodes: node `a`: `context[0]`: artifact.nmae: unknown field `nmae`, expected `node` or `name` at line 3 column 3"
    );
}

#[test]
fn an_on_finish_step_names_its_kind_or_is_refused() {
    let text = err::<Workflow>(&format!("{NODE}on_finish:\n  - clean: worktree\n"));
    assert_eq!(
        text,
        "`on_finish[0]`: on_finish: unknown key `clean` for an `on_finish` step; one of `cleanup`, `distill` at line 7 column 3"
    );
}

#[test]
fn an_artifact_and_a_prompt_file_refuse_unknown_keys() {
    let text = err::<Workflow>(
        "name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: { file: p.md, fil: x }\n",
    );
    assert_eq!(
        text,
        "`nodes[0]`: nodes: node `a`: prompt.fil: unknown field `fil`, expected `file` at line 3 column 3"
    );
    let text = err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    artifacts:\n      produces: [{ name: n, kind: findings, knd: x }]\n");
    assert_eq!(
        text,
        "`nodes[0]`: nodes: node `a`: `artifacts.produces[0]`: artifact.knd: unknown field `knd`, expected `name` or `kind` at line 3 column 3"
    );
}

#[test]
fn a_mode_and_an_input_refuse_unknown_keys() {
    let text = err::<Workflow>(&format!(
        "{NODE}modes:\n  quick: {{ include: [a], includes: [] }}\n"
    ));
    assert_eq!(
        text,
        "`modes.quick.includes`: modes.quick: unknown field `includes`, expected `include` at line 7 column 26"
    );
    let text = err::<Workflow>(&format!(
        "{NODE}inputs:\n  idea: {{ type: string, descripton: x }}\n"
    ));
    assert_eq!(
        text,
        "`inputs.idea`: inputs: unknown field `descripton`, expected one of `required`, `default`, `description`, `pattern`, `min_length` at line 7 column 3"
    );
}

#[test]
fn a_config_layer_refuses_unknown_keys_at_every_level() {
    let text = err::<ConfigLayer>("versio: 1\n");
    assert_eq!(
        text,
        "`versio`: unknown field `versio`, expected one of `version`, `runners`, `adapters`, `mcp_servers`, `project`, `storage`, `paths`, `defaults`, `baseline`, `coverage`, `skills`, `permissions`, `limits`, `pricing`, `forge`, `secrets`"
    );
    let text = err::<ConfigLayer>("defaults:\n  on_failur: pause\n");
    assert_eq!(
        text,
        "`defaults.on_failur`: defaults: unknown field `on_failur`, expected one of `isolation`, `runner`, `timeout_minutes`, `on_failure`, `max_parallel_nodes`, `on_interrupt` at line 2 column 3"
    );
    let text =
        err::<ConfigLayer>("runners:\n  planner:\n    - { adapter: mock, model: m, agnt: x }\n");
    assert_eq!(
        text,
        "`runners.planner[0].agnt`: runners.planner[0]: unknown field `agnt`, expected one of `adapter`, `model`, `agent` at line 3 column 34"
    );
    let text = err::<ConfigLayer>("permissions:\n  commands:\n    denied: [rm]\n");
    assert_eq!(
        text,
        "`permissions.commands.denied`: permissions.commands: unknown field `denied`, expected `deny` or `allow` at line 3 column 5"
    );
}

#[test]
fn a_pack_manifest_refuses_unknown_keys() {
    let text = err::<PackManifest>(
        "name: p\npublisher: acme\nversion: 0.1.0\ndeclares: { permissions: edit, netwrk: false }\n",
    );
    assert_eq!(
        text,
        "`declares.netwrk`: declares: unknown field `netwrk`, expected one of `permissions`, `network`, `executors` at line 4 column 32"
    );
}

#[test]
fn a_ledger_refuses_unknown_keys_on_tasks_and_criteria() {
    let text = err::<Ledger>(
        "tasks:\n  - id: t\n    titel: x\n    scope: [a]\n    criteria: [{ cmd: true }]\n",
    );
    assert_eq!(
        text,
        "`tasks[0].titel`: tasks[0]: unknown field `titel`, expected one of `id`, `title`, `scope`, `criteria`, `depends_on`, `notes`, `manual_review`, `justification` at line 3 column 5"
    );
    let text = err::<Ledger>("tasks:\n  - id: t\n    title: x\n    scope: [a]\n    criteria: [{ cmd: true, typ: guard }]\n");
    assert_eq!(
        text,
        "`tasks[0].criteria[0].typ`: tasks[0].criteria[0]: unknown field `typ`, expected `cmd` or `type` at line 5 column 29"
    );
}

#[test]
fn a_questions_artifact_refuses_unknown_keys() {
    let text = err::<QuestionsFile>("questions:\n  - id: q\n    text: t\n    answer_type: text\n    required: true\n    valeus: []\n");
    assert_eq!(
        text,
        "`questions[0].valeus`: questions[0]: unknown field `valeus`, expected one of `id`, `text`, `answer_type`, `values`, `required` at line 6 column 5"
    );
}

#[test]
fn a_persisted_document_still_tolerates_keys_it_does_not_know() {
    // The engine writes the lock and reads it back with whatever binary
    // comes later: an unknown key there is a newer writer, never a typo.
    let lock: yunta_core::PackLock = yaml::parse("packs: {}\nsignature: abc\n").unwrap();
    assert!(
        lock.packs.is_empty(),
        "a lock with no pack entries reads back empty: {:?}",
        lock.packs
    );
}
