//! The words of an input that does not resolve. The CLI adds the flag that
//! names it; these are the words the problem itself is told in.

use yunta_core::inputs::InputsProblem;
use yunta_core::InputName;

use crate::inputs::problem;

fn name(text: &str) -> InputName {
    InputName::from(text)
}

#[test]
fn an_unknown_input_names_each_one_and_what_the_workflow_declares() {
    let refused = |declared: Vec<InputName>| InputsProblem::Unknown {
        names: vec![name("a"), name("b")],
        declared,
    };
    assert_eq!(
        problem(&refused(vec![name("c")])),
        "unknown inputs `a`, `b` — this workflow declares only: c"
    );
    assert_eq!(
        problem(&refused(Vec::new())),
        "unknown inputs `a`, `b` — this workflow declares no inputs"
    );
}

#[test]
fn a_missing_input_with_no_default_is_named() {
    assert_eq!(
        problem(&InputsProblem::Missing { name: name("goal") }),
        "input `goal` is required and has no default"
    );
}

#[test]
fn a_value_of_the_wrong_kind_says_what_it_must_be() {
    assert_eq!(
        problem(&InputsProblem::InvalidNumber {
            name: name("n"),
            value: "abc".to_string(),
        }),
        "input `n` expects a finite number, got `abc`"
    );
    assert_eq!(
        problem(&InputsProblem::InvalidBoolean {
            name: name("flag"),
            value: "yes".to_string(),
        }),
        "input `flag` expects `true` or `false`, got `yes`"
    );
}

#[test]
fn a_number_outside_its_bounds_says_which_bound_it_broke() {
    assert_eq!(
        problem(&InputsProblem::BelowMin {
            name: name("n"),
            value: 1.0,
            min: 2.0,
        }),
        "input `n` must be >= 2, got 1"
    );
    assert_eq!(
        problem(&InputsProblem::AboveMax {
            name: name("n"),
            value: 9.5,
            max: 5.0,
        }),
        "input `n` must be <= 5, got 9.5"
    );
}

#[test]
fn a_text_that_breaks_its_length_or_its_pattern_or_its_choices_says_so() {
    assert_eq!(
        problem(&InputsProblem::TooShort {
            name: name("s"),
            min_length: 5,
            actual: 2,
        }),
        "input `s` must be at least 5 characters, got 2"
    );
    assert_eq!(
        problem(&InputsProblem::PatternMismatch {
            name: name("p"),
            pattern: "^a".to_string(),
            value: "b".to_string(),
        }),
        "input `p` must match pattern `^a`, got `b`"
    );
    assert_eq!(
        problem(&InputsProblem::NotInEnum {
            name: name("e"),
            value: "z".to_string(),
            values: vec!["a".to_string(), "b".to_string()],
        }),
        "input `e` must be one of [a, b], got `z`"
    );
}

#[test]
fn a_path_that_is_missing_or_unreadable_is_named() {
    assert_eq!(
        problem(&InputsProblem::PathNotFound {
            name: name("f"),
            path: "docs/x.md".to_string(),
        }),
        "input `f` names a path that doesn't exist: `docs/x.md`"
    );
    assert_eq!(
        problem(&InputsProblem::DocumentUnreadable {
            name: name("d"),
            path: "dir".to_string(),
            label: "tasks document".into(),
            detail: "is a directory".to_string(),
        }),
        "input `d` names `dir`, which cannot be read as a file: is a directory — point the input \
         at the tasks document itself"
    );
}

#[test]
fn a_pattern_that_is_not_a_regex_says_why() {
    assert_eq!(
        problem(&InputsProblem::InvalidPattern {
            name: name("p"),
            pattern: "[".to_string(),
            detail: "unclosed character class".to_string(),
        }),
        "input `p`'s pattern `[` is not a valid regex: unclosed character class"
    );
}
