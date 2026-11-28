use yunta_core::{
    AdapterId, ConfigLayer, Detected, ModelName, NonEmpty, ProbedAdapter, RunnerCandidate,
    RunnerName,
};

use super::{config_yaml, DeclaredRunner, ProjectSetup, RunnerDeclaration};
use crate::{ProjectDraft, ProposedRunners, RunnerAnswer};

/// One adapter this machine answered for.
fn claude_answered() -> Vec<ProbedAdapter> {
    vec![ProbedAdapter {
        id: AdapterId::from_static("claude-code"),
        healthy: true,
        detail: "2.1.0".to_string(),
    }]
}

/// The one role a project with no workflow naming one is proposed, its default runner.
fn implementer() -> ProposedRunners {
    ProposedRunners::StandIn
}

/// `name` set up on the main branch, its runners written as `runners` says, answered from
/// a proposal of `implementer` alone.
fn setup(name: &str, runners: RunnerDeclaration) -> ProjectSetup {
    ProjectSetup {
        name: name.to_string(),
        base_branch: "main".to_string(),
        runners,
        proposed: implementer(),
    }
}

/// The name a project is written under, as its config reads it back.
fn read_back(name: &str) -> Option<String> {
    let setup = setup(name, RunnerDeclaration::Proposed);
    let yaml = config_yaml(&setup, &[], &Detected::default());
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
        let yaml = config_yaml(&setup, &[], &Detected::default());
        let project = ConfigLayer::read(&yaml).unwrap().project.unwrap();
        assert_eq!(project.base_branch.as_deref(), Some(branch), "{branch}");
    }
}

/// A name nothing in YAML would cut is written as it is.
#[test]
fn a_plain_name_is_written_as_it_is() {
    let setup = setup("web-app", RunnerDeclaration::Proposed);
    let yaml = config_yaml(&setup, &[], &Detected::default());
    assert!(yaml.contains("\n  name: web-app\n"), "{yaml}");
    assert!(yaml.contains("\n  base_branch: main\n"), "{yaml}");
}

/// Where no adapter answers on this machine, a proposed runner is written commented, on
/// the first adapter this build knows, its model left to that adapter.
#[test]
fn a_runner_no_adapter_answers_for_is_written_commented_on_an_adapter_alone() {
    let unanswered = vec![ProbedAdapter {
        healthy: false,
        detail: "not on PATH".to_string(),
        ..claude_answered().remove(0)
    }];
    let setup = draft_on(unanswered.clone()).answered("", "", Vec::new());
    assert_eq!(setup.runners(), &RunnerDeclaration::Proposed);
    let yaml = config_yaml(&setup, &unanswered, &Detected::default());
    let first_built = crate::machine::first_built_adapter();
    assert!(
        yaml.ends_with(&format!(
            "# `runners:` names roles your workflows' `runner:` fields reference,\n\
             # each with one or more adapter candidates (first capable one wins).\n\
             # not available: claude-code (not on PATH)\n\
             # runners:\n\
             #   implementer:\n\
             #     - {{ adapter: {first_built} }}\n\
             # defaults:\n\
             #   runner: implementer\n"
        )),
        "{yaml}"
    );
    let config = ConfigLayer::read(&yaml).unwrap();
    assert!(
        config.runners.is_none() && config.defaults.is_none(),
        "{yaml}"
    );
}

/// A declared runner is written uncommented and reads back as declared, with the
/// default runner a node naming none resolves to; a model whose characters YAML
/// would read as more than a name reads back whole.
#[test]
fn a_declared_runner_is_written_and_reads_back() {
    let candidate = RunnerCandidate {
        adapter: AdapterId::from_static("claude-code"),
        model: Some("opus,4:1#x".parse::<ModelName>().unwrap()),
        agent: None,
        effort: None,
    };
    let declared = DeclaredRunner {
        runner: RunnerName::PROPOSED,
        candidates: vec![candidate.clone()],
    };
    let setup = setup("web-app", RunnerDeclaration::Declared(vec![declared]));
    let yaml = config_yaml(&setup, &claude_answered(), &Detected::default());
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

/// Claude Code, then Codex, answering on this machine, in the order they were probed.
fn both_answered() -> Vec<ProbedAdapter> {
    let mut probed = claude_answered();
    probed.push(ProbedAdapter {
        id: AdapterId::from_static("codex"),
        healthy: true,
        detail: "0.157.0".to_string(),
    });
    probed
}

/// A draft of `web-app` on a machine that answered as `probed`, proposing `implementer`.
fn draft_on(probed: Vec<ProbedAdapter>) -> ProjectDraft {
    ProjectDraft {
        repo: "/home/me/web-app".into(),
        name: "web-app".to_string(),
        base_branch: "main".to_string(),
        detected: Detected::default(),
        probed,
        proposed: implementer(),
    }
}

/// `adapter`, with `model` where one is named, as a runner's candidate.
fn candidate(adapter: &'static str, model: Option<&str>) -> RunnerCandidate {
    RunnerCandidate {
        adapter: AdapterId::from_static(adapter),
        model: model.map(|model| model.parse::<ModelName>().unwrap()),
        agent: None,
        effort: None,
    }
}

/// The candidates `draft`, answered for `implementer` with `answers`, writes for it, as
/// its configuration reads them back.
fn written_for_implementer(
    draft: &ProjectDraft,
    answers: Vec<RunnerAnswer>,
) -> Vec<RunnerCandidate> {
    let setup = draft.answered("", "", answers);
    let yaml = config_yaml(&setup, &draft.probed, &draft.detected);
    let runners = ConfigLayer::read(&yaml).unwrap().runners;
    runners
        .and_then(|mut runners| runners.remove(&RunnerName::PROPOSED))
        .unwrap_or_default()
}

/// On a machine answering for two adapters, a role answered on the second is declared on
/// both: the one picked first, with the model typed for it, then the other with none.
/// With one adapter answering it is declared on that one alone.
#[test]
fn a_role_is_declared_on_every_adapter_that_answers_the_one_picked_first() {
    let on_codex = RunnerAnswer {
        runner: RunnerName::PROPOSED,
        adapter: AdapterId::from_static("codex"),
        model: Some("gpt-5".parse::<ModelName>().unwrap()),
    };
    assert_eq!(
        written_for_implementer(&draft_on(both_answered()), vec![on_codex]),
        [
            candidate("codex", Some("gpt-5")),
            candidate("claude-code", None)
        ]
    );
    let on_claude = RunnerAnswer {
        runner: RunnerName::PROPOSED,
        adapter: AdapterId::from_static("claude-code"),
        model: Some("opus".parse::<ModelName>().unwrap()),
    };
    assert_eq!(
        written_for_implementer(&draft_on(claude_answered()), vec![on_claude]),
        [candidate("claude-code", Some("opus"))]
    );
}

/// A role nobody answered for, as `init` writes it unasked, is declared on every adapter
/// that answers, in the order they were probed, each with no model: what leaving every
/// question writes too.
#[test]
fn a_role_nobody_answered_is_declared_on_every_adapter_that_answers_in_order() {
    let draft = draft_on(both_answered());
    let unasked = written_for_implementer(&draft, Vec::new());
    assert_eq!(
        unasked,
        [candidate("claude-code", None), candidate("codex", None)]
    );
    let left = RunnerAnswer {
        runner: RunnerName::PROPOSED,
        adapter: AdapterId::from_static("claude-code"),
        model: None,
    };
    assert_eq!(written_for_implementer(&draft, vec![left]), unasked);
}

/// A repository with no workflow is proposed the one role that stands for whatever a
/// node naming no runner runs on: it is declared, and it is the default runner, so a
/// workflow added afterwards whose node names none runs on it.
#[test]
fn a_repository_with_no_workflow_makes_its_stand_in_role_the_default_runner() {
    let draft = ProjectDraft {
        proposed: ProposedRunners::of(&[]),
        ..draft_on(claude_answered())
    };
    let setup = draft.answered("", "", Vec::new());
    let yaml = config_yaml(&setup, &draft.probed, &draft.detected);
    let config = ConfigLayer::read(&yaml).unwrap();
    let default = config.defaults.and_then(|defaults| defaults.runner);
    assert_eq!(default, Some(RunnerName::PROPOSED), "{yaml}");
    assert!(
        yaml.ends_with(
            "runners:\n  implementer:\n    - adapter: claude-code\ndefaults:\n  runner: implementer\n"
        ),
        "{yaml}"
    );
}

/// Where the workflows name their runners, every one is declared, in the order they name
/// them, and the default runner is the first only where a node names none.
#[test]
fn named_roles_are_declared_in_order_with_a_default_only_where_a_node_names_none() {
    let (planner, executor) = (
        RunnerName::from_static("planner"),
        RunnerName::from_static("executor"),
    );
    let named = |needs_default| ProjectDraft {
        proposed: ProposedRunners::Named {
            roles: NonEmpty::from((planner.clone(), vec![executor.clone()])),
            needs_default,
        },
        ..draft_on(claude_answered())
    };
    let written = |draft: &ProjectDraft| {
        let setup = draft.answered("", "", Vec::new());
        config_yaml(&setup, &draft.probed, &draft.detected)
    };
    let every_node_names_one = written(&named(false));
    assert!(
        every_node_names_one.ends_with(
            "runners:\n  planner:\n    - adapter: claude-code\n  executor:\n    \
             - adapter: claude-code\n"
        ),
        "{every_node_names_one}"
    );
    let a_node_names_none = written(&named(true));
    let config = ConfigLayer::read(&a_node_names_none).unwrap();
    let default = config.defaults.and_then(|defaults| defaults.runner);
    assert_eq!(default, Some(planner), "{a_node_names_none}");
}

/// A role answered with an adapter and no model is written declared on that adapter
/// alone, and the configuration reads back with that runner: its model is left to the
/// adapter's CLI.
#[test]
fn a_role_answered_with_an_adapter_and_no_model_is_written_declared() {
    let draft = ProjectDraft {
        repo: "/home/me/web-app".into(),
        name: "web-app".to_string(),
        base_branch: "main".to_string(),
        detected: Detected::default(),
        probed: claude_answered(),
        proposed: implementer(),
    };
    let answer = RunnerAnswer {
        runner: RunnerName::PROPOSED,
        adapter: AdapterId::from_static("claude-code"),
        model: None,
    };
    let setup = draft.answered("", "", vec![answer]);
    let yaml = config_yaml(&setup, &draft.probed, &draft.detected);
    let config = ConfigLayer::read(&yaml).unwrap();
    let runners = config.runners.expect("the role is declared");
    let on_its_adapter = RunnerCandidate {
        adapter: AdapterId::from_static("claude-code"),
        model: None,
        agent: None,
        effort: None,
    };
    assert_eq!(
        runners.get(&RunnerName::PROPOSED),
        Some(&vec![on_its_adapter]),
        "{yaml}"
    );
    assert!(
        yaml.contains("runners:\n  implementer:\n    - adapter: claude-code\n"),
        "{yaml}"
    );
    assert!(!yaml.contains("model"), "{yaml}");
}
