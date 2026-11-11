use std::collections::BTreeMap;

use yunta_core::{
    AdapterId, ConfigLayer, Detected, ModelName, ProbedAdapter, RunnerCandidate, RunnerName,
};

use super::{config_yaml, ProjectSetup, ProposedRunners, RunnerDeclaration};

/// One adapter this machine answered for.
fn claude_answered() -> Vec<ProbedAdapter> {
    vec![ProbedAdapter {
        id: AdapterId::from_static("claude-code"),
        healthy: true,
        detail: "2.1.0".to_string(),
    }]
}

/// The role a project with no workflow naming one is proposed, needed by no node.
fn implementer() -> ProposedRunners {
    ProposedRunners {
        roles: vec![RunnerName::PROPOSED],
        needs_default: false,
    }
}

fn setup(name: &str, runners: RunnerDeclaration) -> ProjectSetup {
    ProjectSetup {
        name: name.to_string(),
        base_branch: "main".to_string(),
        runners,
    }
}

/// The name a project is written under, as its config reads it back.
fn read_back(name: &str) -> Option<String> {
    let setup = setup(name, RunnerDeclaration::Proposed);
    let yaml = config_yaml(&setup, &[], &Detected::default(), &implementer());
    ConfigLayer::read(&yaml).unwrap().project.unwrap().name
}

/// A project's name reads back whole, whatever a directory is called: one a
/// YAML comment or a mapping would cut is quoted.
#[test]
fn a_project_name_reads_back_whole() {
    for name in [
        "my #project",
        "a: b",
        "'quoted'",
        "\"double\"",
        "yes",
        "042",
    ] {
        assert_eq!(read_back(name).as_deref(), Some(name), "{name}");
    }
}

/// A base branch reads back whole, as a name does: one a YAML comment or a mapping
/// would cut is quoted.
#[test]
fn a_base_branch_reads_back_whole() {
    for branch in ["#dev", "a: b", "yes", "042", "release/2.0"] {
        let setup = ProjectSetup {
            base_branch: branch.to_string(),
            ..setup("web-app", RunnerDeclaration::Proposed)
        };
        let yaml = config_yaml(&setup, &[], &Detected::default(), &implementer());
        let project = ConfigLayer::read(&yaml).unwrap().project.unwrap();
        assert_eq!(project.base_branch.as_deref(), Some(branch), "{branch}");
    }
}

/// A name nothing in YAML would cut is written as it is.
#[test]
fn a_plain_name_is_written_as_it_is() {
    let setup = setup("web-app", RunnerDeclaration::Proposed);
    let yaml = config_yaml(&setup, &[], &Detected::default(), &implementer());
    assert!(yaml.contains("\n  name: web-app\n"), "{yaml}");
    assert!(yaml.contains("\n  base_branch: main\n"), "{yaml}");
}

/// A proposed runner is written commented, on the first adapter that answered,
/// its model left for the person to name: as `init` has always written it.
#[test]
fn a_proposed_runner_is_written_commented_as_init_writes_it() {
    let setup = setup("web-app", RunnerDeclaration::Proposed);
    let yaml = config_yaml(
        &setup,
        &claude_answered(),
        &Detected::default(),
        &implementer(),
    );
    assert!(
        yaml.ends_with(
            "# `runners:` names roles your workflows' `runner:` fields reference,\n\
             # each with one or more adapter candidates (first capable one wins).\n\
             # detected: claude-code (2.1.0)\n\
             # runners:\n\
             #   implementer:\n\
             #     - { adapter: claude-code, model: <model-name> }\n"
        ),
        "{yaml}"
    );
    assert!(ConfigLayer::read(&yaml).unwrap().runners.is_none());
}

/// A declared runner is written uncommented and reads back as declared, with the
/// default runner a node naming none resolves to; a model whose characters YAML
/// would read as more than a name reads back whole.
#[test]
fn a_declared_runner_is_written_and_reads_back() {
    let candidate = RunnerCandidate {
        adapter: AdapterId::from_static("claude-code"),
        model: "opus,4:1#x".parse::<ModelName>().unwrap(),
        agent: None,
        effort: None,
    };
    let declared = BTreeMap::from([(RunnerName::PROPOSED, candidate.clone())]);
    let setup = setup("web-app", RunnerDeclaration::Declared(declared));
    let proposed = ProposedRunners {
        needs_default: true,
        ..implementer()
    };
    let yaml = config_yaml(&setup, &claude_answered(), &Detected::default(), &proposed);
    let config = ConfigLayer::read(&yaml).unwrap();
    let runners = config.runners.expect("runners declared");
    assert_eq!(
        runners.get(&RunnerName::PROPOSED),
        Some(&vec![candidate]),
        "{yaml}"
    );
    let defaults = config.defaults.expect("a default runner");
    assert_eq!(defaults.runner, Some(RunnerName::PROPOSED), "{yaml}");
}

/// A role a person left without a model stays proposed, commented beside the ones
/// declared, and no default runner names it.
#[test]
fn a_role_left_without_a_model_stays_proposed_beside_the_declared() {
    let planner = RunnerName::from_static("planner");
    let candidate = RunnerCandidate {
        adapter: AdapterId::from_static("claude-code"),
        model: "opus".parse::<ModelName>().unwrap(),
        agent: None,
        effort: None,
    };
    let declared = BTreeMap::from([(planner.clone(), candidate)]);
    let setup = setup("web-app", RunnerDeclaration::Declared(declared));
    let proposed = ProposedRunners {
        roles: vec![RunnerName::PROPOSED, planner.clone()],
        needs_default: true,
    };
    let yaml = config_yaml(&setup, &claude_answered(), &Detected::default(), &proposed);
    let config = ConfigLayer::read(&yaml).unwrap();
    let runners = config.runners.expect("runners declared");
    assert_eq!(runners.keys().collect::<Vec<_>>(), [&planner], "{yaml}");
    assert!(yaml.contains("  # implementer:\n"), "{yaml}");
    let defaults = config.defaults.map(|defaults| defaults.runner);
    assert_eq!(defaults.flatten(), None, "{yaml}");
}
