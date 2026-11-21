//! The words of a retry: what a menu's retry does and what it costs say what
//! the retry waits for, once, whether the menu offers one retry or several.

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
        failure: Failure::Message {
            outcome: "the linter failed".into(),
        },
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

/// What the retry option of `retry_menu(waits)` does and what it costs.
fn retry_words_of(waits: Vec<Precondition>) -> Option<(Option<String>, Option<String>)> {
    remedy_words(&retry_menu(waits), ReservedOption::Retry)
}

/// The words of a retry that costs `costs`: every retry starts the same way.
fn retried(costs: &str) -> Option<(Option<String>, Option<String>)> {
    let starts = "Starts a fresh attempt from the run's tree as it stands".to_string();
    Some((Some(starts), Some(costs.to_string())))
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
        retried(
            "what failed is asked again and what passed stays passed; until `x` can run, it \
             fails the same way and asks again"
        )
    );
}

#[test]
fn a_retry_on_a_tree_that_already_moved_waits_for_nothing_however_many_say_so() {
    let said = retried("what failed is asked again and what passed stays passed");
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
        retried(
            "what failed is asked again and what passed stays passed; until `x` can run and \
             something outside the tree changes, it fails the same way and asks again"
        )
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
        retried(
            "what failed is asked again and what passed stays passed; until `x` can run, the \
             credential is fixed, and something outside the tree changes, it fails the same \
             way and asks again"
        )
    );
}

#[test]
fn a_tree_that_already_moved_needs_no_waiting_beside_another_input() {
    assert_eq!(
        retry_words_of(vec![
            Precondition::TreeChanged,
            Precondition::OutsideTreeChanged,
        ]),
        retried(
            "what failed is asked again and what passed stays passed; until something outside \
             the tree changes, it fails the same way and asks again"
        )
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
    let (_, tradeoff) = remedy_words(&menu, ReservedOption::Retry).expect("a retry is worded");
    assert!(
        tradeoff
            .as_deref()
            .is_some_and(|tradeoff| tradeoff.ends_with(
                "until something outside the tree changes, it fails the same way and asks again"
            )),
        "{}",
        tradeoff.as_deref().unwrap_or("none")
    );
}

/// What a capped loop's retry does is what it says, on the menu a run writes now and on one
/// an older log holds, which waits for a person to lift the cap.
#[test]
fn a_capped_loop_s_retry_says_its_count_starts_over() {
    for precondition in [None, Some(Precondition::CapLifted)] {
        let menu = FailureMenu {
            attempt: 2,
            failure: Failure::LoopCapped {
                node: yunta_core::NodeId::from("implement"),
                loop_cap: 2,
            },
            remedies: vec![Remedy {
                option: ReservedOption::Retry,
                precondition,
            }],
            stuck: None,
            reroute: None,
        };
        let words =
            remedy_words(&menu, ReservedOption::Retry).expect("a capped loop's retry has words");
        let again = "its iteration count starts over; it stops at the cap again if tasks remain";
        assert_eq!(words, (None, Some(again.to_string())));
    }
}
