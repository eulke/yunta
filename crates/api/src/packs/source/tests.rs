use super::{split_directory, split_source_and_ref, PackSource};

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
    assert_eq!(
        split_source_and_ref("github.com/acme/packs//review@v1.2.0"),
        ("github.com/acme/packs//review", Some("v1.2.0"))
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

/// The directory follows the first `//` after a scheme's own: a URL, a
/// `file://` path, an SSH shorthand and a bare path each name theirs, and the
/// `//` of a scheme alone names none.
#[test]
fn the_directory_follows_the_first_double_slash_after_the_scheme() {
    assert_eq!(
        split_directory("https://github.com/acme/mono//packs/review"),
        ("https://github.com/acme/mono", Some("packs/review"))
    );
    assert_eq!(
        split_directory("file:///srv/mono//packs/review"),
        ("file:///srv/mono", Some("packs/review"))
    );
    assert_eq!(
        split_directory("git@github.com:acme/mono.git//packs/review"),
        ("git@github.com:acme/mono.git", Some("packs/review"))
    );
    assert_eq!(
        split_directory("../mono//packs/review"),
        ("../mono", Some("packs/review"))
    );
    assert_eq!(
        split_directory("https://github.com/acme/review-pack"),
        ("https://github.com/acme/review-pack", None)
    );
    assert_eq!(
        split_directory("/srv/review-pack"),
        ("/srv/review-pack", None)
    );
}

/// The lock records the repository as git clones it, then the directory, which
/// an update reads back as the same source; a source with no directory, or a
/// `//` naming nothing, records the repository alone.
#[test]
fn a_recorded_source_reads_back_as_itself() {
    let named = |spec: &str| PackSource::read(spec).ok().map(|source| source.recorded());
    assert_eq!(
        named("github.com/acme/mono//packs/./review"),
        Some("https://github.com/acme/mono//packs/review".to_string())
    );
    assert_eq!(
        named("https://github.com/acme/mono//packs/review"),
        Some("https://github.com/acme/mono//packs/review".to_string())
    );
    assert_eq!(
        named("github.com/acme/review-pack"),
        Some("https://github.com/acme/review-pack".to_string())
    );
    assert_eq!(named("../mono//"), Some("../mono".to_string()));
}
