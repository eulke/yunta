use chrono::TimeZone;

use super::*;
use crate::events::{CriterionResult, LimitSource, RedCheck, SessionRefusal, UnpassableTask};
use crate::ids::{AdapterId, TaskId};

/// The facts of a failure at `failed_at` with nothing else moved: no
/// continuation, no grant and an unchanged tree.
fn facts_at(failed_at: DateTime<Utc>) -> Facts {
    Facts {
        failed_at,
        continuable: false,
        may_grant: false,
        tree_moved: false,
    }
}

/// A session refused the request for `cause`.
fn refused(cause: RefusalCause) -> Failure {
    Failure::SessionRefused {
        session_refused: SessionRefusal {
            adapter: AdapterId::from_static("mock"),
            cause,
            message: "Invalid API key".into(),
        },
    }
}

/// The one option a retry waiting for `precondition` offers.
fn retry_waiting_for(precondition: Precondition) -> Outlook {
    Outlook::Choose(vec![Remedy {
        option: ReservedOption::Retry,
        precondition: Some(precondition),
    }])
}

/// A loop whose one task stopped because its command cannot run.
fn cannot_run() -> Failure {
    let run = CriterionResult {
        cmd: "yunta-no-such-tool".into(),
        exit_code: 127,
        r#type: None,
        reused: false,
        duration_ms: None,
        output: None,
        tail: Vec::new(),
        tree: None,
        head: None,
        conclusion: None,
    };
    Failure::TasksStopped {
        stopped: vec![TaskStop {
            task_id: TaskId::from_static("greet"),
            cause: TaskCause::Unrunnable { runs: vec![run] },
        }],
    }
}

/// A plan whose one criterion no work can pass.
fn unpassable_plan() -> Failure {
    Failure::PlanUnpassable {
        unpassable: vec![UnpassableTask {
            task_id: TaskId::from_static("task-u"),
            cmd: "false".into(),
            why: "false exits 1".into(),
        }],
    }
}

#[test]
fn a_failure_offers_only_what_can_change_its_outcome() {
    let failed_at = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
    let facts = facts_at(failed_at);
    let red = Failure::Red {
        red: RedCheck::Coverage {
            cmd: "make coverage".into(),
            measured: Some(92.5),
            threshold: 95.0,
        },
    };
    assert_eq!(red.remedies(&facts), Outlook::Stuck(Stuck::TreeUnchanged));
    assert_eq!(
        refused(RefusalCause::Final).remedies(&facts),
        Outlook::Stuck(Stuck::Final)
    );
    assert_eq!(
        refused(RefusalCause::Authentication).remedies(&facts),
        retry_waiting_for(Precondition::CredentialFixed)
    );
    let later = failed_at + chrono::Duration::hours(1);
    let waits = Failure::Limit {
        limit: AccountLimit {
            source: LimitSource::Forge,
            resets_at: Some(later),
            message: "rate limited".into(),
        },
    };
    assert_eq!(waits.remedies(&facts), Outlook::WaitUntil { at: later });
    // The way out of an unrunnable command is the command: no continuation is offered.
    let continuable = Facts {
        continuable: true,
        ..facts.clone()
    };
    assert_eq!(
        cannot_run().remedies(&continuable),
        retry_waiting_for(Precondition::CommandRunnable {
            cmd: "yunta-no-such-tool".into(),
        })
    );
    assert_eq!(
        unpassable_plan().remedies(&facts),
        Outlook::Stuck(Stuck::CriteriaWrong)
    );
    assert_eq!(
        Failure::unchanged(1, unpassable_plan()).remedies(&facts),
        Outlook::Stuck(Stuck::CriteriaWrong)
    );
}

/// A loop whose one task stopped at its pre-check with `found`.
fn pre_checked(found: Vec<Surprise>) -> Failure {
    Failure::TasksStopped {
        stopped: vec![TaskStop {
            task_id: TaskId::from_static("greet"),
            cause: TaskCause::PreCheck {
                found: NonEmpty::new(found).unwrap(),
            },
        }],
    }
}

/// A criterion whose command cannot run: the shell answers 127 for a command it cannot find.
fn unrunnable(cmd: &str) -> Surprise {
    Surprise::Unrunnable {
        cmd: cmd.into(),
        exit_code: 127,
    }
}

#[test]
fn a_pre_check_offers_a_command_fix_for_each_distinct_unrunnable_command() {
    let failed_at = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
    let facts = facts_at(failed_at);
    assert_eq!(
        pre_checked(vec![unrunnable("yunta-no-such-tool")]).remedies(&facts),
        retry_waiting_for(Precondition::CommandRunnable {
            cmd: "yunta-no-such-tool".into(),
        })
    );
    assert_eq!(
        pre_checked(vec![
            unrunnable("yunta-first"),
            unrunnable("yunta-second"),
            unrunnable("yunta-first"),
        ])
        .remedies(&facts),
        Outlook::Choose(vec![
            Remedy {
                option: ReservedOption::Retry,
                precondition: Some(Precondition::CommandRunnable {
                    cmd: "yunta-first".into(),
                }),
            },
            Remedy {
                option: ReservedOption::Retry,
                precondition: Some(Precondition::CommandRunnable {
                    cmd: "yunta-second".into(),
                }),
            },
        ])
    );
}

#[test]
fn a_trivial_criterion_keeps_a_pre_check_stuck_wherever_it_stands() {
    let failed_at = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
    let facts = facts_at(failed_at);
    let trivial = Surprise::TrivialCriterion { cmd: "true".into() };
    let missing = unrunnable("yunta-no-such-tool");
    let broken = Surprise::BrokenGuard {
        cmd: "test -f seed".into(),
        proves: None,
    };
    let found = [
        vec![trivial.clone()],
        vec![missing.clone(), trivial.clone()],
        vec![trivial.clone(), missing],
        vec![broken, trivial],
    ];
    for surprises in found {
        let outlook = pre_checked(surprises.clone()).remedies(&facts);
        assert_eq!(
            outlook,
            Outlook::Stuck(Stuck::CriteriaWrong),
            "{surprises:?}"
        );
    }
}

#[test]
fn a_broken_guard_alone_waits_for_the_outside_until_the_tree_moved() {
    let failed_at = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
    let guard = |cmd: &str| Surprise::BrokenGuard {
        cmd: cmd.into(),
        proves: None,
    };
    let broken = pre_checked(vec![guard("test -f seed")]);
    assert_eq!(
        broken.remedies(&facts_at(failed_at)),
        retry_waiting_for(Precondition::OutsideTreeChanged)
    );
    let moved = Facts {
        tree_moved: true,
        ..facts_at(failed_at)
    };
    assert_eq!(
        broken.remedies(&moved),
        retry_waiting_for(Precondition::TreeChanged)
    );
    // Every red guard shares the one retry: two of them offer it once.
    let two = pre_checked(vec![guard("test -f seed"), guard("test -f other")]);
    assert_eq!(
        two.remedies(&facts_at(failed_at)),
        retry_waiting_for(Precondition::OutsideTreeChanged)
    );
}

#[test]
fn a_broken_guard_beside_an_unrunnable_command_offers_both_remedies() {
    let failed_at = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
    let retry = |precondition: Precondition| Remedy {
        option: ReservedOption::Retry,
        precondition: Some(precondition),
    };
    let command = Precondition::CommandRunnable {
        cmd: "yunta-no-such-tool".into(),
    };
    let found = pre_checked(vec![
        Surprise::BrokenGuard {
            cmd: "test -f seed".into(),
            proves: None,
        },
        unrunnable("yunta-no-such-tool"),
    ]);
    assert_eq!(
        found.remedies(&facts_at(failed_at)),
        Outlook::Choose(vec![
            retry(command.clone()),
            retry(Precondition::OutsideTreeChanged),
        ])
    );
    let moved = Facts {
        tree_moved: true,
        ..facts_at(failed_at)
    };
    assert_eq!(
        found.remedies(&moved),
        Outlook::Choose(vec![retry(command), retry(Precondition::TreeChanged)])
    );
}

#[test]
fn a_continuation_is_listed_before_the_retry_it_sits_beside() {
    let failed_at = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
    let facts = Facts {
        continuable: true,
        tree_moved: true,
        ..facts_at(failed_at)
    };
    let unmet = Failure::TasksStopped {
        stopped: vec![TaskStop {
            task_id: TaskId::from_static("T001"),
            cause: TaskCause::Unmet {
                attempts: 1,
                red: Vec::new(),
                outside: Vec::new(),
            },
        }],
    };
    assert_eq!(
        unmet.remedies(&facts),
        Outlook::Choose(vec![
            Remedy {
                option: ReservedOption::ContinueWork,
                precondition: None,
            },
            Remedy {
                option: ReservedOption::Retry,
                precondition: Some(Precondition::TreeChanged),
            },
        ])
    );
}

/// A stop that names no run names no command to fix, so the outside decides.
#[test]
fn a_stop_that_names_no_run_waits_on_the_outside() {
    let stop = Failure::TasksStopped {
        stopped: vec![TaskStop {
            task_id: TaskId::from_static("greet"),
            cause: TaskCause::Unrunnable { runs: Vec::new() },
        }],
    };
    assert_eq!(
        stop.remedies(&facts_at(
            Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap()
        )),
        retry_waiting_for(Precondition::OutsideTreeChanged)
    );
}

#[test]
fn a_carried_stop_is_stuck_on_the_tree_until_the_tree_moved() {
    let failed_at = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
    let carried = Failure::TasksStopped {
        stopped: vec![TaskStop {
            task_id: TaskId::from_static("greet"),
            cause: TaskCause::CarriedWorkNoLongerApplies {
                paths: vec!["greet.txt".into()],
            },
        }],
    };
    assert_eq!(
        carried.remedies(&facts_at(failed_at)),
        Outlook::Stuck(Stuck::TreeUnchanged),
        "a retry on the tree the carried work conflicts with stops the same way"
    );
    let moved = Facts {
        tree_moved: true,
        ..facts_at(failed_at)
    };
    assert_eq!(
        carried.remedies(&moved),
        retry_waiting_for(Precondition::TreeChanged)
    );
}
