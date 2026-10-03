//! `optional: true` — a node a run may leave out when the project lacks
//! what it needs, declared only where leaving it out leaves the run
//! whole.

use std::path::Path;

use yunta_core::workflow::read::read;
use yunta_core::{left_out, Because, CommandName, ConfigKey, ConfigLayer, LeftOut, Workflow};

const PATH: &str = ".yunta/workflows/w.yaml";

fn refused_codes(text: &str) -> Vec<String> {
    read(text, Path::new(PATH))
        .expect_err("the workflow is refused")
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn workflow(text: &str) -> Workflow {
    read(text, Path::new(PATH)).expect("the workflow reads")
}

/// A lint the project may not have, and the fix only a failing lint
/// leads to.
const LINTED: &str = "\
name: w
nodes:
  - { id: build, kind: bash, run: \"true\" }
  - id: lint
    kind: bash
    run: { command: lint }
    optional: true
    depends_on: [build]
    on_failure: { goto: fix, max_reroutes: 1 }
  - { id: fix, kind: prompt, runner: executor, prompt: fix it, context: [{ node-output: { node: lint } }] }
  - { id: ship, kind: bash, run: \"true\", depends_on: [lint] }
";

#[test]
fn optional_is_refused_on_a_parallel_child() {
    let grouped = "name: w\nnodes:\n  - id: checks\n    kind: parallel\n    nodes:\n      - { id: lint, kind: bash, run: { command: lint }, optional: true }\n      - { id: docs, kind: bash, run: \"true\" }\n";
    assert_eq!(refused_codes(grouped), ["incoherent-optional"]);

    workflow("name: w\nnodes:\n  - id: checks\n    kind: parallel\n    optional: true\n    nodes:\n      - { id: lint, kind: bash, run: { command: lint } }\n");
}

#[test]
fn a_required_node_never_reroutes_to_an_optional_one() {
    let rerouting = "name: w\nnodes:\n  - { id: fix, kind: bash, run: { command: fix }, optional: true }\n  - { id: test, kind: bash, run: \"true\", on_failure: { goto: fix, max_reroutes: 1 } }\n";
    assert_eq!(refused_codes(rerouting), ["incoherent-optional"]);
}

#[test]
fn a_required_reader_of_an_optional_node_is_refused_unless_only_it_leads_there() {
    workflow(LINTED);

    let reading = "name: w\nnodes:\n  - { id: lint, kind: bash, run: { command: lint }, optional: true }\n  - { id: report, kind: bash, run: \"true\", depends_on: [lint], context: [{ node-output: { node: lint } }] }\n";
    assert_eq!(refused_codes(reading), ["incoherent-optional"]);
}

#[test]
fn left_out_names_the_lacking_keys_and_what_only_it_led_to() {
    let linted = workflow(LINTED);

    assert_eq!(
        left_out(&linted, &ConfigLayer::default()),
        vec![
            LeftOut {
                node: "lint".into(),
                because: Because::Lacks {
                    lacks: vec![ConfigKey::Command {
                        command: CommandName::from("lint")
                    }]
                },
            },
            LeftOut {
                node: "fix".into(),
                because: Because::Through {
                    through: "lint".into()
                },
            },
        ]
    );
    let declared: ConfigLayer =
        yunta_core::yaml::parse("commands: { lint: \"pnpm lint\" }").expect("the config parses");
    assert_eq!(left_out(&linted, &declared), vec![]);
}

#[test]
fn a_left_out_node_says_what_the_project_lacks() {
    let lacks = Because::Lacks {
        lacks: vec![ConfigKey::Command {
            command: CommandName::from("lint"),
        }],
    };
    assert_eq!(lacks.to_string(), "the project declares no command `lint`");
    assert_eq!(
        Because::Through {
            through: "lint".into()
        }
        .to_string(),
        "only `lint` leads to it, and it is not in this run"
    );
}
