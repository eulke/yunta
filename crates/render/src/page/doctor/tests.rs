use std::time::Duration;

use surco::components::{Fix, Found};
use yunta_core::{AdapterId, RunnerName};

use super::finding::{checkouts_check, first_run_check, versioned};
use super::{checks, CheckoutsKeep, Finding, PackLack, Readiness, SessionOutcome};

#[test]
fn a_healthy_adapter_folds_as_its_id_and_the_version_it_reported() {
    let claude = AdapterId::from_static("claude-code");
    assert_eq!(
        versioned(&claude, Some("2.1.305 (Claude Code)")),
        "claude-code 2.1.305"
    );
    assert_eq!(
        versioned(&AdapterId::from_static("codex"), Some("codex-cli 0.157.0")),
        "codex 0.157.0"
    );
    assert_eq!(versioned(&claude, None), "claude-code");
}

#[test]
fn checkouts_within_their_budget_hold() {
    let check = checkouts_check("2 checkouts", CheckoutsKeep::Within);
    assert_eq!(check.found, Found::Holds);
    assert!(check.fix.is_none(), "{check:?}");
}

#[test]
fn builds_past_their_budget_are_worth_a_look_and_say_how_to_empty_them() {
    let check = checkouts_check("2 checkouts", CheckoutsKeep::BuildsOverBudget);
    assert_eq!(check.found, Found::Caution);
    assert_eq!(
        check.so.as_deref(),
        Some("the next checkout taken empties the builds past the budget")
    );
    assert_eq!(
        check.fix.map(|fix| fix.said).as_deref(),
        Some("run `yunta gc` to empty them now")
    );
}

#[test]
fn a_disk_under_what_it_keeps_free_stops_a_run_and_says_how_to_make_room() {
    let check = checkouts_check("2 checkouts", CheckoutsKeep::DiskUnderFree);
    assert_eq!(check.found, Found::Problem);
    assert_eq!(
        check.fix.map(|fix| fix.said).as_deref(),
        Some("make room (`yunta gc`, close a parked run, or lower `storage.pool.keep_free`)")
    );
}

#[test]
fn a_first_run_that_costs_little_more_holds() {
    let check = first_run_check(Duration::from_millis(30), Duration::from_millis(8), false);
    assert_eq!(check.found, Found::Holds);
    assert_eq!(
        check.said,
        "a new binary's first run takes 30 ms here, its next 8 ms"
    );
    assert!(check.fix.is_none(), "{check:?}");
}

#[test]
fn a_first_run_that_costs_a_suite_minutes_says_how_to_exempt_the_terminal() {
    let check = first_run_check(Duration::from_millis(450), Duration::from_millis(7), true);
    assert_eq!(check.found, Found::Caution);
    assert_eq!(check.subject, "first run");
    assert_eq!(
        check.said,
        "a new binary's first run takes 450 ms here, its next 7 ms"
    );
    assert!(check.so.is_some_and(|so| so.contains("macOS assesses")));
    assert!(check
        .fix
        .is_some_and(|fix| fix.said.contains("Developer Tools")));
}

#[test]
fn a_fix_is_said_once_under_the_first_row_that_needs_it() {
    let declare = Fix::said("declare a runner in .yunta/config.yaml:");
    let lacking = |pack: &str| Finding::PackLacks {
        pack: pack.parse().unwrap(),
        lack: PackLack::Runner {
            runner: RunnerName::from_static("planner"),
            declare: declare.clone(),
        },
    };
    let rows = checks(&[lacking("acme/review"), lacking("acme/spec")]);
    let fixes: Vec<bool> = rows.iter().map(|check| check.fix.is_some()).collect();
    assert_eq!(fixes, [true, false]);
}

#[test]
fn a_machine_is_ready_only_when_every_check_holds() {
    let named = Finding::Committer {
        who: "Ada <ada@example.com>".to_string(),
    };
    let unnamed = Finding::NoCommitter {
        git_said: "no user.name".to_string(),
    };
    assert_eq!(
        Readiness::of(std::slice::from_ref(&named)),
        Readiness::Ready
    );
    assert_eq!(Readiness::of(&[named, unnamed]), Readiness::NotReady);
}

#[test]
fn a_session_that_ended_well_folds_under_its_binding() {
    let candidate = yunta_core::RunnerCandidate {
        adapter: AdapterId::from_static("claude-code"),
        model: "claude-sonnet-5".parse().unwrap(),
        agent: None,
        effort: None,
    };
    let session = Finding::Session {
        candidate,
        named_by: vec![
            (RunnerName::from_static("executor"), true),
            (RunnerName::from_static("reviewer"), false),
        ],
        outcome: SessionOutcome::Ok { tokens: 812 },
    };
    let check = session.check();
    assert_eq!(
        check.subject,
        "claude-code/claude-sonnet-5 (executor, reviewer fallback)"
    );
    assert_eq!(check.said, "ok: 812 tokens");
    assert_eq!(check.brief.as_deref(), Some("claude-code/claude-sonnet-5"));
}
