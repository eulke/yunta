use super::split_source_and_ref;

/// A pin follows the source's path, after its last `@`.
#[test]
fn a_pin_after_the_path_is_the_ref() {
    assert_eq!(
        split_source_and_ref("github.com/acme/review-pack@v1.2.0"),
        ("github.com/acme/review-pack", Some("v1.2.0"))
    );
    assert_eq!(
        split_source_and_ref("git@github.com:acme/review-pack.git@v1"),
        ("git@github.com:acme/review-pack.git", Some("v1"))
    );
}

/// The `@` of an SSH shorthand is the user's, never a pin: before the path,
/// or, with no `/` at all, followed by the `host:` git never takes in a ref.
#[test]
fn the_at_of_an_ssh_shorthand_is_never_a_pin() {
    assert_eq!(
        split_source_and_ref("git@github.com:acme/review-pack.git"),
        ("git@github.com:acme/review-pack.git", None)
    );
    assert_eq!(
        split_source_and_ref("git@host:review-pack.git"),
        ("git@host:review-pack.git", None)
    );
}
