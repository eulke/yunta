//! The words of a retry: the label and the tradeoff a menu's retry carries
//! say what the retry waits for, once, whether the menu offers one retry or
//! several.

use yunta_core::events::{
    Failure, FailureMenu, Precondition, Remedy, Surprise, TaskCause, TaskStop,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::NonEmpty;

use crate::blocks::remedy_words;

/// The menu of a failure on its second attempt, offering one retry for each of `waits`.
fn retry_menu(waits: Vec<Precondition>) -> FailureMenu {
    FailureMenu {
        attempt: 2,
        failure: Failure::message("the linter failed"),
        remedies: waits
            .into_iter()
            .map(|precondition| Remedy {
                option: ReservedOption::Retry,
                precondition: Some(precondition),
            })
            .collect(),
        stuck: None,
        reroute: None,
    }
}

/// The label and the tradeoff the retry option of `retry_menu(waits)` carries.
fn retry_words_of(waits: Vec<Precondition>) -> Option<(String, Option<String>)> {
    remedy_words(&retry_menu(waits), ReservedOption::Retry)
}

/// A command that has to run before a retry can end differently.
fn runnable(cmd: &str) -> Precondition {
    Precondition::CommandRunnable {
        cmd: cmd.to_string(),
    }
}

#[test]
fn a_single_retry_names_its_one_input() {
    assert_eq!(
        retry_words_of(vec![runnable("x")]),
        Some((
            "Run it again (attempt 2) once `x` can run".to_string(),
            Some(
                "Starts a fresh attempt from the run's tree as it stands, under the same \
                 scope: what failed is asked again and what passed stays passed. Until `x` \
                 can run, it fails the same way and asks again"
                    .to_string()
            ),
        ))
    );
}

#[test]
fn a_retry_on_a_tree_that_already_moved_says_the_tree_changed_however_many_say_so() {
    let said = Some((
        "Run it again (attempt 2): the tree changed since it failed".to_string(),
        Some(
            "Starts a fresh attempt from the run's tree as it stands, under the same \
             scope: what failed is asked again and what passed stays passed; if the change \
             does not fix what it failed on, it fails the same way and asks again"
                .to_string(),
        ),
    ));
    assert_eq!(retry_words_of(vec![Precondition::TreeChanged]), said);
    assert_eq!(
        retry_words_of(vec![Precondition::TreeChanged, Precondition::TreeChanged]),
        said
    );
}

#[test]
fn retries_that_wait_for_several_inputs_say_them_in_one_clause() {
    assert_eq!(
        retry_words_of(vec![runnable("x"), Precondition::OutsideTreeChanged]),
        Some((
            "Run it again (attempt 2) once `x` can run and something outside the tree changes"
                .to_string(),
            Some(
                "Starts a fresh attempt from the run's tree as it stands, under the same \
                 scope: what failed is asked again and what passed stays passed. Until `x` \
                 can run and something outside the tree changes, it fails the same way and \
                 asks again"
                    .to_string()
            ),
        ))
    );
}

#[test]
fn three_inputs_are_joined_with_a_comma_before_the_last_and() {
    assert_eq!(
        retry_words_of(vec![
            runnable("x"),
            Precondition::CredentialFixed,
            Precondition::OutsideTreeChanged,
        ]),
        Some((
            "Run it again (attempt 2) once `x` can run, the credential is fixed, and \
             something outside the tree changes"
                .to_string(),
            Some(
                "Starts a fresh attempt from the run's tree as it stands, under the same \
                 scope: what failed is asked again and what passed stays passed. Until `x` \
                 can run, the credential is fixed, and something outside the tree changes, \
                 it fails the same way and asks again"
                    .to_string()
            ),
        ))
    );
}

#[test]
fn a_tree_that_already_moved_needs_no_waiting_beside_another_input() {
    assert_eq!(
        retry_words_of(vec![
            Precondition::TreeChanged,
            Precondition::OutsideTreeChanged,
        ]),
        Some((
            "Run it again (attempt 2) once something outside the tree changes".to_string(),
            Some(
                "Starts a fresh attempt from the run's tree as it stands, under the same \
                 scope: what failed is asked again and what passed stays passed. Until \
                 something outside the tree changes, it fails the same way and asks again"
                    .to_string()
            ),
        ))
    );
}

#[test]
fn an_input_that_several_retries_name_is_named_once() {
    assert_eq!(
        retry_words_of(vec![
            Precondition::OutsideTreeChanged,
            Precondition::OutsideTreeChanged,
        ]),
        retry_words_of(vec![Precondition::OutsideTreeChanged]),
    );
}

/// A loop whose task's pre-check found a guard red before any work, on a tree
/// that has not moved: the stop names the guard, and the retry names the one
/// input that can change it.
#[test]
fn a_red_guard_is_named_by_its_stop_and_its_retry_waits_on_the_outside() {
    let guard = Surprise::BrokenGuard {
        cmd: "test -f seed".into(),
        proves: None,
    };
    let stopped = Failure::TasksStopped {
        stopped: vec![TaskStop {
            task_id: "greet".into(),
            cause: TaskCause::PreCheck {
                found: NonEmpty::from((guard, Vec::new())),
            },
        }],
    };
    let said = crate::failure::sentence(&stopped);
    assert!(
        said.contains("guard `test -f seed` is already red before any work started"),
        "{said}"
    );
    let menu = FailureMenu {
        attempt: 2,
        failure: stopped,
        remedies: vec![Remedy {
            option: ReservedOption::Retry,
            precondition: Some(Precondition::OutsideTreeChanged),
        }],
        stuck: None,
        reroute: None,
    };
    let (label, tradeoff) = remedy_words(&menu, ReservedOption::Retry).expect("a retry is worded");
    assert_eq!(
        label,
        "Run it again (attempt 2) once something outside the tree changes"
    );
    assert!(
        tradeoff
            .as_deref()
            .is_some_and(|tradeoff| tradeoff.ends_with(
                "Until something outside the tree changes, it fails the same way and asks again"
            )),
        "{}",
        tradeoff.as_deref().unwrap_or("none")
    );
}
