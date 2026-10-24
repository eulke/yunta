//! Every document a person writes refuses a key the schema does not
//! know, naming the key and where it sits, with what is valid there or the
//! key it most likely meant.

use std::path::Path;

use yunta_core::diagnostic::RuleCode;
use yunta_core::workflow::read::read_all;
use yunta_core::yaml;
use yunta_core::{ConfigLayer, PackManifest, QuestionsFile, TasksFile, Workflow};

fn err<T: serde::de::DeserializeOwned>(text: &str) -> String {
    match yaml::parse::<T>(text) {
        Ok(_) => panic!("parsed a document with an unknown key:\n{text}"),
        Err(e) => e.to_string(),
    }
}

/// What reading a workflow refuses it for: each problem as a line of text
/// says it, the rule it broke, and what helps fix it.
fn refused(text: &str) -> Vec<(String, Option<RuleCode>, Vec<String>)> {
    let (_, report) = read_all(text, Path::new("w.yaml"));
    report
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.to_string(), diagnostic.rule(), diagnostic.help()))
        .collect()
}

const NODE: &str = "name: w\nnodes:\n  - id: a\n    kind: bash\n    run: \"true\"\n";

#[test]
fn a_workflow_refuses_an_unknown_top_level_key() {
    assert_eq!(
        refused(&format!("{NODE}nodez: []\n")),
        [(
            "the document has an unknown key `nodez`; did you mean `nodes`? at line 6 column 1"
                .to_string(),
            Some(RuleCode::UnknownKey),
            vec!["did you mean `nodes`?".to_string()],
        )]
    );
}

/// What a slip was read as is judged as meant, where the person wrote it:
/// `depend_on` names a node there is none of.
#[test]
fn a_node_refuses_every_unknown_key_where_it_is_written() {
    assert_eq!(
        refused(
            "name: w\nnodes:\n  - id: plan\n    kind: prompt\n    prompt: p\n    depend_on: [a]\n    scpe: []\n",
        ),
        [
            (
                "node `plan`: unknown key `depend_on` for a `prompt` node; did you mean \
                 `depends_on`? at line 6 column 5"
                    .to_string(),
                Some(RuleCode::UnknownKey),
                vec!["did you mean `depends_on`?".to_string()],
            ),
            (
                "node `plan`: `depends_on` names `a`, and no node carries that id at line 6 \
                 column 17"
                    .to_string(),
                Some(RuleCode::UnknownDependency),
                Vec::new(),
            ),
            (
                "node `plan`: unknown key `scpe` for a `prompt` node; did you mean `scope`? at \
                 line 7 column 5"
                    .to_string(),
                Some(RuleCode::UnknownKey),
                vec!["did you mean `scope`?".to_string()],
            ),
        ]
    );
}

#[test]
fn a_node_key_that_belongs_to_another_kind_is_refused() {
    assert_eq!(
        refused("name: w\nnodes:\n  - id: a\n    kind: bash\n    run: x\n    prompt: p\n"),
        [(
            "node `a`: unknown key `prompt` for a `bash` node at line 6 column 5".to_string(),
            Some(RuleCode::UnknownKey),
            Vec::new(),
        )]
    );
}

#[test]
fn role_and_fresh_context_are_refused_with_the_key_that_replaces_them() {
    let found = refused(
        "name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    role: planner\n    fresh_context: true\n",
    );
    let help: Vec<&[String]> = found.iter().map(|(_, _, help)| help.as_slice()).collect();
    assert_eq!(
        help,
        [
            &["a node names its runner with `runner:`".to_string()][..],
            &[
                "every session starts fresh; `on_interrupt: resume_session` reuses one only \
               when a run resumes"
                    .to_string()
            ][..],
        ],
        "{found:#?}"
    );
    assert!(
        found[0]
            .0
            .starts_with("node `a`: unknown key `role` for a `prompt` node")
            && found[0].0.ends_with("at line 6 column 5"),
        "{found:#?}"
    );
}

/// `interactive` said how a node's questions were presented; a node that
/// declares `questions` says everything there is to say, and whichever
/// surface is watching decides the rest.
#[test]
fn interactive_is_refused_with_the_reason_it_no_longer_exists() {
    let found = refused(
        "name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    interactive: true\n",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].0.contains("unknown key `interactive`")
            && found[0].2
                == [
                    "a node that declares `questions` asks them, and whichever surface is \
                     watching puts them to a person"
                ],
        "{found:#?}"
    );
}

#[test]
fn a_context_entry_names_its_source_or_is_refused() {
    let text = err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    context:\n      - filez: [x]\n");
    assert_eq!(
        text,
        "`nodes[0]`: node `a`: `context[0]`: unknown key `filez` for a context source; one of `files`, `command`, `artifact`, `mcp`, `run-events`, `tasks`, `knowledge`, `node-output`; did you mean `files`? at line 3 column 5"
    );
    let text = err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    context:\n      - artifact: { node: b, name: n, nmae: x }\n");
    assert_eq!(
        text,
        "`nodes[0]`: node `a`: `context[0]`: artifact: unknown key `nmae` for an \
         `artifact:` context source; one of `node`, `kind`, `name` at line 3 column 5"
    );
}

#[test]
fn an_on_finish_step_names_its_kind_or_is_refused() {
    let text = err::<Workflow>(&format!("{NODE}on_finish:\n  - clean: worktree\n"));
    assert_eq!(
        text,
        "`on_finish[0]`: unknown key `clean` for an `on_finish` step; one of `cleanup`, `distill`; did you mean `cleanup`? at line 7 column 5"
    );
}

#[test]
fn an_artifact_and_a_prompt_file_refuse_unknown_keys() {
    let text = err::<Workflow>(
        "name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: { file: p.md, fil: x }\n",
    );
    assert_eq!(
        text,
        "`nodes[0]`: node `a`: prompt.fil: unknown field `fil`, expected `file` at line 3 column 5"
    );
    // An artifact is one bare string, so anything else is refused with
    // the two shapes a string can be rather than a key listing.
    let text = err::<Workflow>("name: w\nnodes:\n  - id: a\n    kind: prompt\n    prompt: p\n    artifacts:\n      produces: [{ name: n, kind: findings }]\n");
    assert_eq!(
        text,
        "`nodes[0]`: node `a`: `artifacts.produces[0]`: an artifact is a file name, or \
         one of `tasks`, `spec`, `findings`, `questions`, `answers` for a document the engine reads, \
         not a mapping at line 3 column 5"
    );
}

#[test]
fn a_mode_and_an_input_refuse_unknown_keys() {
    let text = err::<Workflow>(&format!(
        "{NODE}modes:\n  alpha: {{ include: [a], includes: [] }}\n"
    ));
    assert_eq!(
        text,
        "`modes.alpha.includes`: unknown field `includes`, expected `include` or `ask`; did you mean `include`? at line 7 column 26"
    );
    let text = err::<Workflow>(&format!(
        "{NODE}inputs:\n  idea: {{ type: string, descripton: x }}\n"
    ));
    assert_eq!(
        text,
        "`inputs.idea`: unknown field `descripton`, expected one of `required`, `default`, `description`, `pattern`, `min_length`; did you mean `description`? at line 7 column 3"
    );
}

#[test]
fn a_config_layer_refuses_unknown_keys_at_every_level() {
    let text = err::<ConfigLayer>("versio: 1\n");
    assert_eq!(
        text,
        "`versio`: unknown field `versio`, expected one of `version`, `runners`, `adapters`, `mcp_servers`, `project`, `storage`, `paths`, `defaults`, `commands`, `format`, `shared_dirs`, `baseline`, `coverage`, `skills`, `permissions`, `limits`, `pricing`, `forge`, `secrets`; did you mean `version`? at line 1 column 1"
    );
    let text = err::<ConfigLayer>("defaults:\n  on_failur: pause\n");
    assert_eq!(
        text,
        "`defaults.on_failur`: unknown field `on_failur`, expected one of `isolation`, `runner`, `timeout_minutes`, `on_failure`, `max_parallel_nodes`, `on_interrupt`; did you mean `on_failure`? at line 2 column 3"
    );
    let text =
        err::<ConfigLayer>("runners:\n  planner:\n    - { adapter: mock, model: m, agnt: x }\n");
    assert_eq!(
        text,
        "`runners.planner[0].agnt`: unknown field `agnt`, expected one of `adapter`, `model`, `agent`, `effort`; did you mean `agent`? at line 3 column 34"
    );
    let text = err::<ConfigLayer>("permissions:\n  commands:\n    denied: [rm]\n");
    assert_eq!(
        text,
        "`permissions.commands.denied`: unknown field `denied`, expected `deny` or `allow` at line 3 column 5"
    );
}

#[test]
fn a_pack_manifest_refuses_unknown_keys() {
    let text = err::<PackManifest>(
        "name: p\npublisher: acme\nversion: 0.1.0\ndeclares: { permissions: edit, netwrk: false }\n",
    );
    assert_eq!(
        text,
        "`declares.netwrk`: unknown field `netwrk`, expected one of `permissions`, `network`, `executors`; did you mean `network`? at line 4 column 32"
    );
}

#[test]
fn a_tasks_document_refuses_unknown_keys_on_tasks_and_criteria() {
    let text = err::<TasksFile>(
        "tasks:\n  - id: t\n    titel: x\n    scope: [a]\n    criteria: [{ cmd: true }]\n",
    );
    assert_eq!(
        text,
        "`tasks[0].titel`: unknown field `titel`, expected one of `id`, `title`, `scope`, `criteria`, `depends_on`, `notes`, `description`, `changes`, `outcome`, `uses`, `invariants`; did you mean `title`? at line 3 column 5"
    );
    let text = err::<TasksFile>("tasks:\n  - id: t\n    title: x\n    scope: [a]\n    criteria: [{ cmd: true, typ: guard }]\n");
    assert_eq!(
        text,
        "`tasks[0].criteria[0].typ`: unknown field `typ`, expected one of `cmd`, `type`, `proves`; did you mean `type`? at line 5 column 29"
    );
}

/// A task's `criteria` are its whole verification, and what no command can
/// settle goes behind a `gate`, where a person decides. A document that asks
/// for a judge instead is refused with the key it wrote.
#[test]
fn a_task_that_asks_to_be_reviewed_by_hand_is_refused_with_the_key_it_wrote() {
    let text = err::<TasksFile>(
        "tasks:\n  - id: t\n    title: x\n    scope: [a]\n    criteria: [{ cmd: \"cargo test\" }]\n    manual_review: true\n    justification: \"no command reads prose\"\n",
    );
    assert_eq!(
        text,
        "`tasks[0].manual_review`: unknown field `manual_review`, expected one of `id`, `title`, `scope`, `criteria`, `depends_on`, `notes`, `description`, `changes`, `outcome`, `uses`, `invariants` at line 6 column 5"
    );
}

#[test]
fn a_questions_artifact_refuses_unknown_keys() {
    let text = err::<QuestionsFile>("questions:\n  - id: q\n    text: t\n    answer_type: text\n    required: true\n    valeus: []\n");
    assert_eq!(
        text,
        "`questions[0].valeus`: unknown field `valeus`, expected one of `id`, `text`, `answer_type`, `values`, `required`, `assumes`; did you mean `values`? at line 6 column 5"
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
