//! The words of a grant: what it lets be written, and whose scope it
//! widens, the node's or, for a loop that ended owing its tasks' requests,
//! each task's own.

use yunta_core::events::{Failure, FailureMenu, OwedScope, Remedy};
use yunta_core::reserved::ReservedOption;
use yunta_core::{ScopeGlob, TaskId};

use crate::blocks::remedy_words;

/// The menu of a failure on its second attempt that offers a grant.
fn grant_menu(failure: Failure) -> FailureMenu {
    FailureMenu {
        attempt: 2,
        failure,
        remedies: vec![Remedy {
            option: ReservedOption::Grant,
            precondition: None,
        }],
        stuck: None,
        reroute: None,
    }
}

/// What task `task` asked to write, and why.
fn owed(task: &str, paths: &[&str]) -> OwedScope {
    OwedScope {
        task_id: TaskId::from(task),
        paths: paths.iter().map(|path| ScopeGlob::from(*path)).collect(),
        reason: Some("the call sites are there".to_string()),
    }
}

#[test]
fn a_grant_of_what_a_task_owes_names_the_task_and_its_paths() {
    let menu = grant_menu(Failure::ScopeOwed {
        owed: vec![owed("resolve", &["src/a.rs", "src/b.rs"])],
    });
    let (label, tradeoff) =
        remedy_words(&menu, ReservedOption::Grant).expect("a menu's grant has words");
    assert_eq!(
        label,
        "Allow task `resolve` to also write src/a.rs, src/b.rs and run it again (attempt 2)"
    );
    let tradeoff = tradeoff.expect("a grant says what it widens");
    assert!(tradeoff.contains("each task's scope"), "{tradeoff}");
}

#[test]
fn a_grant_of_what_several_tasks_owe_counts_them() {
    let menu = grant_menu(Failure::ScopeOwed {
        owed: vec![owed("one", &["src/a.rs"]), owed("two", &["src/b.rs"])],
    });
    let (label, _) = remedy_words(&menu, ReservedOption::Grant).expect("a menu's grant has words");
    assert_eq!(
        label,
        "Allow each of 2 tasks to also write what it asked for and run them again (attempt 2)"
    );
}

#[test]
fn an_owed_request_a_red_criterion_located_says_so_in_its_line() {
    let failure = Failure::ScopeOwed {
        owed: vec![OwedScope {
            task_id: TaskId::from("resolve"),
            paths: vec![ScopeGlob::from("src/a.rs")],
            reason: None,
        }],
    };
    assert_eq!(
        crate::failure::detail(&failure),
        vec!["`resolve` — src/a.rs: a red criterion of the task locates the refused writes"]
    );
}
