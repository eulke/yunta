//! What lies past a decision's options, as a run froze it: a `pull_request` node opens its
//! pull request on the forge and the repository the config declares, and a run whose config
//! declares no forge leaves that node out, which is all that is said of it.

use std::path::Path;

use yunta_core::ahead::{Ahead, NodesAhead};
use yunta_core::workflow::read::read;
use yunta_core::{left_out, Because, ConfigKey, ConfigLayer, NodeId, Workflow};

/// A gate whose approve goes on to the node that opens the run's pull request.
const SHIPPING: &str = "\
name: w
nodes:
  - { id: build, kind: bash, run: \"true\" }
  - id: ship
    kind: gate
    assignee: lead
    message: Create the PR?
    depends_on: [build]
    options: [approve]
  - { id: pr, kind: pull_request, title: the change, optional: true, depends_on: [ship] }
";

/// What lies ahead at `node` of `SHIPPING`, under the config `config` says.
fn ahead_at(config: &str, node: &str) -> Option<Ahead> {
    let workflow: Workflow = read(SHIPPING, Path::new(".yunta/workflows/w.yaml")).unwrap();
    let config: ConfigLayer = yunta_core::yaml::parse(config).unwrap();
    let left = left_out(&workflow, &config);
    NodesAhead::of(&workflow, &config, &left)
        .at(&NodeId::from(node))
        .cloned()
}

#[test]
fn a_pull_request_node_opens_it_on_the_forge_the_config_declares() {
    let forge = "forge:\n  github: { repo: eulke/yunta, token_env: FORGE_TOKEN }\n";
    let Some(Ahead::OpensPullRequest { repo }) = ahead_at(forge, "pr") else {
        panic!("the pull request node opens it");
    };
    assert_eq!(repo.to_string(), "eulke/yunta");
    assert_eq!(ahead_at(forge, "build"), None);
}

#[test]
fn with_no_forge_declared_the_pull_request_node_is_left_out() {
    let lacks = vec![ConfigKey::Forge];
    assert_eq!(
        ahead_at("{}\n", "pr"),
        Some(Ahead::LeftOut(Because::Lacks { lacks }))
    );
}
