//! What every persisted document promises, stated once so each crate
//! asserts it of the documents it owns rather than restating the rule.

use yunta_core::persisted::{Persisted, PersistedDoc};

/// `document` is written with its own version, read back as itself, and
/// a file from a newer writer is refused naming both versions.
///
/// Every persisted document owes the same three things, so the three
/// are asserted here and each crate calls this of the documents it
/// declares — the test that would otherwise be written once per type,
/// slightly differently each time.
///
/// # Panics
///
/// When any of the three fails, which is what a test wants.
#[allow(clippy::panic, clippy::expect_used)]
pub fn holds_its_version<T: Persisted + std::fmt::Debug + PartialEq + Clone>(document: T) {
    assert!(
        T::SCHEMA_VERSION >= 1,
        "`{}` is written down, so it carries a version from its first commit",
        T::NAME
    );

    let bytes = PersistedDoc::of(document.clone())
        .write()
        .expect("a persisted document writes");

    let read = PersistedDoc::<T>::read(&bytes).expect("and reads back");
    assert_eq!(read.doc, document, "`{}` round-trips", T::NAME);
    // An unstamped file reads as 0, so reading its own version back is
    // what proves the writer stamped it.
    assert_eq!(
        read.schema_version,
        T::SCHEMA_VERSION,
        "`{}` stamps its own version",
        T::NAME
    );
    assert!(
        read.unknown.is_empty(),
        "`{}` understands everything it wrote: {:?}",
        T::NAME,
        read.unknown_keys()
    );

    let mut ahead: serde_json::Value =
        yunta_core::yaml::parse_bytes(&bytes).expect("what it wrote reads back as data");
    if let Some(object) = ahead.as_object_mut() {
        object.insert(
            T::VERSION_KEY.to_string(),
            serde_json::Value::from(T::SCHEMA_VERSION + 1),
        );
    }
    let ahead = serde_json::to_string(&ahead).expect("a file from a newer writer");
    let refused = PersistedDoc::<T>::read(ahead.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("`{}` from a newer writer is refused", T::NAME))
        .to_string();
    assert!(
        refused.contains(&(T::SCHEMA_VERSION + 1).to_string())
            && refused.contains(&T::SCHEMA_VERSION.to_string()),
        "the refusal names what it found and what this binary reads: {refused}"
    );
}
