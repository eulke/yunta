//! What core settles about a request before any file is read.

use yunta_core::expansion::{
    screen, Ask, Bounds, Decision, Denial, Precheck, Proposal, Reach, Screen,
};
use yunta_core::protected::{Protected, Protection};
use yunta_core::{ScopeExpansionMode, ScopeGlob};

#[test]
fn a_criterion_that_could_not_run_settles_the_request_as_a_denial() {
    let paths = [ScopeGlob::from("src/a.rs")];
    let protected = Protected::default();
    let ask = Ask::Requested {
        paths: &paths,
        reason: "fix",
        proposal: Some(Proposal {
            cmd: "cargo test",
            precheck: Precheck::CouldNotRun,
        }),
    };
    let bounds = Bounds {
        mode: ScopeExpansionMode::Rules,
        max_files: 5,
        max_per_run: None,
        protected: &protected,
        beside: &[],
    };
    assert_eq!(
        screen(&ask, &bounds),
        Screen::Settled(Decision::Deny(Denial::CriterionCouldNotRun))
    );
}

#[test]
fn a_refusal_names_each_path_with_the_reason_it_is_protected() {
    let reached = vec![
        Reach {
            path: ScopeGlob::from(".github/ci.yml"),
            reason: Protection::ProjectDeny,
        },
        Reach {
            path: ScopeGlob::from(".claude/settings.json"),
            reason: Protection::AgentConfig,
        },
    ];
    assert_eq!(
        Denial::ReachesProtected { reached }.to_string(),
        "it reaches what the task may never write: .github/ci.yml (the project denies it \
         to every run: permissions.paths.deny), .claude/settings.json (an agent CLI's own \
         configuration)"
    );
    let unnoted = Denial::Chosen {
        option: "deny".to_string(),
        note: None,
    };
    assert_eq!(unnoted.to_string(), "denied by a human at the gate");
    let noted = Denial::Chosen {
        option: "deny".to_string(),
        note: Some("not this sprint".to_string()),
    };
    assert_eq!(noted.to_string(), "not this sprint");
}
