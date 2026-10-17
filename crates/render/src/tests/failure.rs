//! The words of a failure: one test per phrase a log, a menu and a
//! surface read a failure as.

use yunta_core::diagnostic::{
    ArtifactFailure, Diagnostic, DocumentRef, Fault, FileProblem, Named, Problem, Report, Subject,
};
use yunta_core::events::{
    AccountLimit, ArtifactId, CommandExit, CommandOrigin, Failure, HookPhase, LimitSource,
    RedCheck, RefusalCause, SessionRefusal, TaskCause, TaskStop, UnpassableTask,
};
use yunta_core::protected::Protection;
use yunta_core::{AdapterId, ArtifactKind, NodeId, RunId, TaskFault, TaskId};

use crate::failure::{detail, headline, sentence};

fn exited(code: i32, origin: Option<CommandOrigin>, tail: &[&str]) -> CommandExit {
    CommandExit {
        origin,
        code,
        tail: tail.iter().map(|line| line.to_string()).collect(),
        output: None,
    }
}

fn exit_failure(code: i32, tail: &[&str]) -> Failure {
    Failure::Exited {
        exited: exited(code, None, tail),
    }
}

const RUSTC_TAIL: [&str; 2] = [
    "error[E0425]: cannot find value `x` in this scope",
    "--> src/lib.rs:3:5",
];

fn plan() -> DocumentRef {
    DocumentRef::new(ArtifactKind::Tasks, "artifacts/plan.yaml")
}

fn broke(fault: impl Into<Fault>) -> Diagnostic {
    Diagnostic::new(
        Subject::Task(Named::new(TaskId::from("t1"), 0)),
        Problem::broke(fault),
    )
}

#[test]
fn a_command_that_exits_says_its_code() {
    assert_eq!(sentence(&exit_failure(101, &[])), "exit 101");
}

#[test]
fn a_command_that_exits_says_the_last_lines_it_printed() {
    let failure = exit_failure(101, &RUSTC_TAIL);
    assert_eq!(
        sentence(&failure),
        "exit 101: error[E0425]: cannot find value `x` in this scope\n--> src/lib.rs:3:5"
    );
    assert_eq!(headline(&failure), "exit 101");
    assert_eq!(detail(&failure), RUSTC_TAIL);
}

#[test]
fn an_executor_that_exits_is_named_by_the_executor() {
    let failure = Failure::Exited {
        exited: exited(
            101,
            Some(CommandOrigin::Executor {
                executor: "probe".to_string(),
            }),
            &[],
        ),
    };
    assert_eq!(headline(&failure), "executor `probe` exited 101");
}

#[test]
fn a_hook_that_fails_is_named_by_its_phase_and_command() {
    let failure = Failure::Exited {
        exited: exited(
            1,
            Some(CommandOrigin::Hook {
                phase: HookPhase::Before,
                command: "cargo fmt --check".to_string(),
            }),
            &[],
        ),
    };
    assert_eq!(headline(&failure), "before hook `cargo fmt --check` failed");
}

#[test]
fn a_command_that_prints_nothing_reads_as_its_code_alone() {
    assert_eq!(sentence(&exit_failure(1, &[])), "exit 1");
}

#[test]
fn an_unchanged_attempt_says_what_the_attempt_it_repeats_failed_with() {
    let failure = Failure::unchanged(1, exit_failure(101, &[]));
    assert_eq!(
        sentence(&failure),
        "not run again: nothing in the run's tree changed since attempt 1 failed on it — \
         change what it failed on there first. Attempt 1 failed: exit 101"
    );
}

#[test]
fn an_unchanged_attempt_keeps_the_last_lines_of_the_failure_it_repeats() {
    let failure = Failure::unchanged(1, exit_failure(101, &RUSTC_TAIL));
    assert!(
        sentence(&failure).ends_with("--> src/lib.rs:3:5"),
        "{}",
        sentence(&failure)
    );
    assert!(
        detail(&failure).contains(&"--> src/lib.rs:3:5".to_string()),
        "{}",
        detail(&failure).join("\n")
    );
}

#[test]
fn a_scope_violation_names_which_files_fell_outside() {
    let failure =
        Failure::scope_violated(vec!["crates/cli/Cargo.toml".into(), "clippy.toml".into()]);
    assert_eq!(
        sentence(&failure),
        "scope violated: 2 files outside the declared globs — crates/cli/Cargo.toml, clippy.toml"
    );
}

#[test]
fn a_failure_renders_one_block_per_failing_document() {
    let failure = Failure::artifacts(vec![
        ArtifactFailure::file(
            "artifacts/notes.md",
            FileProblem::Missing {
                node: NodeId::from("plan"),
            },
        ),
        ArtifactFailure::Content(Report::new(plan(), vec![broke(TaskFault::NoCriteria)])),
    ]);
    assert_eq!(
        sentence(&failure),
        "artifacts/notes.md: 1 error\n  \
         the document was declared by node `plan` and never produced\n\
         artifacts/plan.yaml: 1 error\n  \
         task `t1`: no criteria declared; every task needs at least one command that verifies it"
    );
}

#[test]
fn an_artifact_another_run_owes_renders_as_the_run_that_does_not_hold_it() {
    let failure = Failure::artifacts(vec![ArtifactFailure::Unheld {
        run: RunId::from("run-child-1"),
        producer: None,
        artifact: ArtifactId::of("report.md", None),
    }]);
    assert_eq!(
        sentence(&failure),
        "run `run-child-1`: 1 error\n  \
         holds no artifact `report.md` — produce it there, or stop declaring it here"
    );
}

#[test]
fn a_denial_read_from_a_log_without_reasons_names_its_paths() {
    let failure = Failure::PathsDenied {
        denied_paths: vec!["a.txt".into(), ".claude/x".into()],
        reasons: Vec::new(),
    };
    assert_eq!(
        sentence(&failure),
        "wrote what no session of the run may write — a.txt, .claude/x: undo those changes"
    );
}

#[test]
fn a_denial_says_why_each_path_is_denied() {
    let failure = Failure::PathsDenied {
        denied_paths: vec!["a.txt".into(), ".claude/x".into()],
        reasons: vec![Protection::ProjectDeny, Protection::AgentConfig],
    };
    assert_eq!(
        sentence(&failure),
        "wrote what no session of the run may write — a.txt (denied by the project's \
         permissions), .claude/x (an agent CLI's own configuration): undo those changes"
    );
}

#[test]
fn an_unpassable_plan_names_each_task_its_criterion_and_why() {
    let failure = Failure::PlanUnpassable {
        unpassable: vec![UnpassableTask {
            task_id: "task-u".into(),
            cmd: "false".to_string(),
            why: "false exits 1".to_string(),
        }],
    };
    assert_eq!(
        headline(&failure),
        "the plan holds a criterion no work can pass — task `task-u`: `false` (false exits 1)"
    );
}

#[test]
fn a_limit_says_which_session_hit_it_and_until_when() {
    let failure = Failure::Limit {
        limit: AccountLimit {
            source: LimitSource::Session {
                adapter: AdapterId::from_static("claude-code"),
            },
            resets_at: Some(yunta_testkit_core::fixed_now()),
            message: "You've hit your usage limit".to_string(),
        },
    };
    assert_eq!(
        sentence(&failure),
        "session `claude-code` hit a limit until 2026-01-01 00:00 UTC: You've hit your usage limit"
    );
}

#[test]
fn a_forge_limit_without_a_reset_says_only_what_the_forge_said() {
    let failure = Failure::Limit {
        limit: AccountLimit {
            source: LimitSource::Forge,
            resets_at: None,
            message: "rate limited".to_string(),
        },
    };
    assert_eq!(
        sentence(&failure),
        "the forge rate-limited its requests: rate limited"
    );
}

#[test]
fn a_refused_session_says_its_message_and_why_it_refused() {
    let failure = Failure::SessionRefused {
        session_refused: SessionRefusal {
            adapter: AdapterId::from_static("mock"),
            cause: RefusalCause::Authentication,
            message: "Invalid API key".to_string(),
        },
    };
    assert_eq!(
        sentence(&failure),
        "session `mock` refused the request (its credential was refused): Invalid API key"
    );
}

#[test]
fn a_red_command_says_how_it_exited_and_what_it_concluded() {
    let failure = Failure::Red {
        red: RedCheck::Command {
            cmd: "cargo test".to_string(),
            exit_code: 101,
            conclusion: Some("test result: FAILED".to_string()),
            output: None,
        },
    };
    assert_eq!(headline(&failure), "`cargo test` exits 101");
    assert_eq!(
        sentence(&failure),
        "`cargo test` exits 101: test result: FAILED"
    );
}

#[test]
fn a_coverage_below_its_threshold_says_both_numbers() {
    let failure = Failure::Red {
        red: RedCheck::Coverage {
            cmd: "make coverage".to_string(),
            measured: Some(92.5),
            threshold: 95.0,
        },
    };
    assert_eq!(
        sentence(&failure),
        "coverage 92.5% is below the 95% threshold"
    );
}

#[test]
fn a_coverage_command_with_no_figure_names_itself() {
    let failure = Failure::Red {
        red: RedCheck::Coverage {
            cmd: "make coverage".to_string(),
            measured: None,
            threshold: 95.0,
        },
    };
    assert_eq!(
        headline(&failure),
        "`make coverage` produced no `NN[.NN]%` coverage figure in its stdout"
    );
    assert_eq!(sentence(&failure), headline(&failure));
}

#[test]
fn a_regressed_suite_says_its_exit_and_the_last_lines_it_printed() {
    let tail = [
        "test a ... FAILED",
        "cat: marker.txt: No such file or directory",
    ];
    let failure = Failure::Red {
        red: RedCheck::Regression {
            cmd: "cargo test".to_string(),
            exit_code: 1,
            tail: tail.iter().map(|line| line.to_string()).collect(),
        },
    };
    assert_eq!(
        headline(&failure),
        "regression: `cargo test` passed at baseline (exit 0) but now exits 1"
    );
    assert_eq!(
        sentence(&failure),
        "regression: `cargo test` passed at baseline (exit 0) but now exits 1: \
         test a ... FAILED\ncat: marker.txt: No such file or directory"
    );
    assert_eq!(detail(&failure), tail);
}

#[test]
fn a_regressed_suite_that_printed_nothing_reads_as_its_headline() {
    let failure = Failure::Red {
        red: RedCheck::Regression {
            cmd: "cargo test".to_string(),
            exit_code: 1,
            tail: Vec::new(),
        },
    };
    assert_eq!(sentence(&failure), headline(&failure));
    assert!(detail(&failure).is_empty());
}

#[test]
fn a_loop_that_stopped_its_tasks_says_each_task_and_why() {
    let failure = Failure::TasksStopped {
        stopped: vec![TaskStop {
            task_id: TaskId::from("greet"),
            cause: TaskCause::ScopeDecisionOwed,
        }],
    };
    assert_eq!(
        sentence(&failure),
        "no task is ready and not all are done — blocked or failed tasks need a decision; \
         task `greet` blocked: a scope expansion request needs a human decision"
    );
}

#[test]
fn a_failed_child_run_names_its_run_and_what_failed_it() {
    let failure = Failure::ChildFailed {
        child_run: RunId::from("child"),
        child_failure: Some(Box::new(exit_failure(1, &[]))),
    };
    assert_eq!(headline(&failure), "child run `child` failed: exit 1");
    assert_eq!(sentence(&failure), "child run `child` failed:\n  exit 1");
}
