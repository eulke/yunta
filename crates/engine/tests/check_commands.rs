//! `check` reads a project's command where the workflow names it: every
//! name the project does not declare is refused before the first token,
//! and the rest is judged as the project wrote it.

use yunta_core::{CommandName, ConfigKey, ConfigLayer, Workflow};
use yunta_engine::CheckError;

fn parse<T: serde::de::DeserializeOwned>(text: &str) -> T {
    yunta_core::yaml::parse(text).expect("the fixture parses")
}

/// What `check` refuses `workflow` for under `config`, as `(node, key)`.
fn refused_keys(workflow: &str, config: &str) -> Vec<(String, ConfigKey)> {
    let workflow: Workflow = parse(workflow);
    let config: ConfigLayer = parse(config);
    yunta_engine::check(&workflow, &config, &|_| None)
        .into_iter()
        .filter_map(|error| match error {
            CheckError::Unset { node, key } => Some((node.to_string(), key)),
            _ => None,
        })
        .collect()
}

fn command(name: &str) -> ConfigKey {
    ConfigKey::Command {
        command: CommandName::from(name),
    }
}

#[test]
fn check_refuses_every_command_a_node_names_and_the_project_lacks() {
    let workflow = "name: w\nnodes:\n  - id: lint\n    kind: bash\n    run: { command: lint }\n    hooks: { before: [{ run: { command: install } }] }\n";

    assert_eq!(
        refused_keys(workflow, "{}"),
        vec![
            ("lint".to_string(), command("install")),
            ("lint".to_string(), command("lint")),
        ]
    );
    assert_eq!(
        refused_keys(
            workflow,
            "commands: { lint: \"pnpm lint\", install: \"pnpm install\" }"
        ),
        vec![]
    );
}

#[test]
fn node_defaults_hooks_naming_a_missing_command_are_refused() {
    let workflow = "name: w\nnode_defaults:\n  hooks: { after: [{ run: { command: fmt } }] }\nnodes:\n  - { id: a, kind: bash, run: \"true\" }\n  - { id: b, kind: bash, run: \"true\" }\n";

    assert_eq!(
        refused_keys(workflow, "{}"),
        vec![("node_defaults".to_string(), command("fmt"))]
    );
}

#[test]
fn programs_named_resolves_a_project_command() {
    let workflow: Workflow =
        parse("name: w\nnodes:\n  - { id: lint, kind: bash, run: { command: lint } }\n");
    let config: ConfigLayer = parse("commands: { lint: \"eslint . --max-warnings 0\" }");

    let named = yunta_engine::programs_named(&workflow, &config);

    assert_eq!(named, vec![("lint".into(), "eslint".to_string())]);
    assert_eq!(
        yunta_engine::programs_named(&workflow, &ConfigLayer::default()),
        vec![]
    );
}

#[test]
fn a_denied_project_command_is_refused_as_the_project_wrote_it() {
    let workflow: Workflow =
        parse("name: w\nnodes:\n  - { id: clean, kind: bash, run: { command: wipe } }\n");
    let config: ConfigLayer = parse(
        "commands: { wipe: \"rm -rf build\" }\npermissions:\n  commands:\n    deny: [\"rm -rf*\"]\n",
    );

    let errors = yunta_engine::check(&workflow, &config, &|_| None);

    assert!(
        errors
            .iter()
            .any(|error| matches!(error, CheckError::CommandDenied { node, .. } if node.as_str() == "clean")),
        "{errors:?}"
    );
}
