//! The identifiers' own tests: the rules each type checks and the text it
//! prints.

use super::*;

#[test]
fn a_handle_is_the_random_tail_of_a_ulid() {
    let made = ulid::Ulid::from_parts(1_700_000_000_000, 0x0123_4567_89AB_CDEF);
    let run = RunId::from(made);
    let random = made.to_string();
    assert_eq!(run.handle(), &random[random.len() - RunId::HANDLE_CHARS..]);
    // Two runs made in the same millisecond share their head and
    // differ in their handle.
    let twin = RunId::from(ulid::Ulid::from_parts(
        1_700_000_000_000,
        0x0123_4567_89AB_CDEE,
    ));
    assert_ne!(run.handle(), twin.handle());
    assert_eq!(RunId::from_static("run-1").handle(), "run-1");
}

#[test]
fn a_name_starts_with_a_letter_and_continues_with_letters_digits_underscore_or_dash() {
    assert!(is_name(b"a"));
    assert!(is_name(b"review-alt_2"));
    assert!(!is_name(b""));
    assert!(!is_name(b"2fast"));
    assert!(!is_name(b"a.b"));
    assert!(!is_name(b"a b"));
}

#[test]
fn a_node_id_admits_exactly_one_fan_out_separator_between_two_names() {
    assert!(is_node_id(b"review"));
    assert!(is_node_id(b"review@alt"));
    assert!(!is_node_id(b"review@"));
    assert!(!is_node_id(b"@alt"));
    assert!(!is_node_id(b"a@b@c"));
}

#[test]
fn a_segment_is_a_printable_word_that_names_one_directory() {
    assert!(is_segment(b"acme"));
    assert!(is_segment(b"run-1"));
    assert!(!is_segment(b"."));
    assert!(!is_segment(b".."));
    assert!(!is_segment(b"a/b"));
    assert!(!is_segment(b"a\\b"));
    assert!(!is_segment(b"a b"));
    assert!(!is_segment(b""));
}

#[test]
fn a_static_identifier_borrows_and_a_parsed_one_owns_but_they_compare_equal() {
    static STATIC: AdapterId = AdapterId::from_static("codex");
    let parsed: AdapterId = "codex".parse().unwrap();
    assert_eq!(STATIC, parsed);
    let mut set = std::collections::HashSet::new();
    set.insert(parsed);
    assert!(set.contains(&STATIC));
    assert!(set.contains("codex"));
}

#[test]
fn debug_names_the_type_around_the_value() {
    let id: NodeId = "plan".parse().unwrap();
    assert_eq!(format!("{id:?}"), "NodeId(\"plan\")");
}
