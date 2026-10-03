//! A workflow runs a project's command by name: the name is the
//! workflow's, the text the project's, and a node says which names it
//! needs before any project reads it.

use yunta_core::yaml;
use yunta_core::{CommandName, ConfigKey, ConfigLayer, NodeKind, RunCommand, Workflow};

fn workflow(yaml_text: &str) -> Workflow {
    yaml::parse(yaml_text).expect("the workflow parses")
}

fn config(yaml_text: &str) -> ConfigLayer {
    yaml::parse(yaml_text).expect("the config parses")
}

fn command(name: &str) -> ConfigKey {
    ConfigKey::Command {
        command: CommandName::from(name),
    }
}

#[test]
fn a_run_is_a_script_or_a_project_command_and_nothing_else() {
    let parsed = workflow(
        "name: w\nnodes:\n  - { id: a, kind: bash, run: \"make lint\" }\n  - { id: b, kind: bash, run: { command: lint } }\n",
    );
    let runs: Vec<&RunCommand> = parsed
        .nodes
        .iter()
        .map(|node| match &node.kind {
            NodeKind::Bash { run } => run,
            other => panic!("a bash node, not {other:?}"),
        })
        .collect();
    assert_eq!(runs[0], &RunCommand::Script("make lint".to_string()));
    assert_eq!(runs[1], &RunCommand::Project(CommandName::from("lint")));

    for refused in [
        "run: 3",
        "run: { command: lint, args: x }",
        "run: { file: lint.sh }",
        "run: { command: \"2lint\" }",
    ] {
        let text = format!("name: w\nnodes:\n  - id: a\n    kind: bash\n    {refused}\n");
        assert!(
            yaml::parse::<Workflow>(&text).is_err(),
            "`{refused}` is neither a script nor a command name"
        );
    }
}

#[test]
fn a_run_command_round_trips() {
    let text = "name: w\nnodes:\n  - id: a\n    kind: bash\n    run: { command: lint }\n    hooks: { after: [{ run: \"echo done\" }, { run: { command: fmt } }] }\n";
    let parsed = workflow(text);
    let again: Workflow =
        yaml::parse(&yaml::to_string(&parsed).expect("it serializes")).expect("it parses back");
    assert_eq!(again, parsed);
}

#[test]
fn commands_merge_key_by_key_across_layers() {
    let org = config("commands: { test: \"make test\", lint: \"make lint\" }\n");
    let repo = config("commands: { lint: \"pnpm lint\" }\n");

    let merged = ConfigLayer::merge_layers([org, repo]);

    assert_eq!(merged.command(&"lint".into()), Some("pnpm lint"));
    assert_eq!(merged.command(&"test".into()), Some("make test"));
    assert_eq!(merged.command(&"format".into()), None);
}

/// Hooks before, the node's own command, hooks after — each name once.
#[test]
fn a_node_needs_every_command_it_names_once_in_the_order_it_meets_them() {
    let parsed = workflow(
        "name: w\nnodes:\n  - id: a\n    kind: bash\n    run: { command: lint }\n    hooks:\n      before: [{ run: { command: install } }, { run: \"echo x\" }]\n      after: [{ run: { command: lint } }, { run: { command: fmt } }]\n",
    );
    let node = &parsed.nodes[0];

    assert_eq!(
        ConfigKey::needed_by(node),
        vec![command("install"), command("lint"), command("fmt")]
    );
    assert_eq!(
        ConfigKey::unset(node, &config("commands: { lint: \"pnpm lint\" }\n")),
        vec![command("install"), command("fmt")]
    );
}
