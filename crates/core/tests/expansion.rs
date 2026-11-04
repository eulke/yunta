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
    let refused = Denial::ReachesProtected {
        reached: reached.clone(),
    };
    assert!(
        matches!(&refused, Denial::ReachesProtected { reached: named } if *named == reached),
        "{refused:?}"
    );
    let unnoted = Denial::Chosen {
        option: "deny".to_string(),
        note: None,
    };
    assert!(
        matches!(&unnoted, Denial::Chosen { option, note: None } if option == "deny"),
        "{unnoted:?}"
    );
    let noted = Denial::Chosen {
        option: "deny".to_string(),
        note: Some("not this sprint".to_string()),
    };
    assert!(
        matches!(&noted, Denial::Chosen { note: Some(note), .. } if note == "not this sprint"),
        "{noted:?}"
    );
}

#[test]
fn a_denial_reads_as_one_sentence_naming_what_protects_each_path() {
    let refused = Denial::ReachesProtected {
        reached: vec![
            Reach {
                path: ScopeGlob::from(".github/ci.yml"),
                reason: Protection::ProjectDeny,
            },
            Reach {
                path: ScopeGlob::from(".git/config"),
                reason: Protection::GitRegister,
            },
        ],
    };
    assert_eq!(
        refused.sentence(),
        "it reaches what the task may never write: .github/ci.yml (denied to every run by \
         `permissions.paths.deny`), .git/config (the run's git register)"
    );
    assert_eq!(
        Denial::PrecheckPasses.sentence(),
        "proposed criterion already passes, so there is nothing to expand for"
    );
    let unnoted = Denial::Chosen {
        option: "deny".to_string(),
        note: None,
    };
    assert_eq!(unnoted.sentence(), "denied by a human at the gate");
}
