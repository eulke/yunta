//! The run's object store: where the bytes of every artifact live, named
//! by what they hash to.
//!
//! Identity is content, not a path. Two writes of the same bytes are one
//! object; an object whose content stopped matching its name is a fact
//! the run reports rather than something a reader silently believes; and
//! the `artifacts/` directory is a view the store writes from what it
//! holds.

use yunta_core::sha256_hex;
use yunta_engine::{ObjectError, ObjectStore};

/// A run directory with the two directories a store writes through.
fn run_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(dir.path().join("scratch")).expect("scratch");
    std::fs::create_dir(dir.path().join("artifacts")).expect("artifacts");
    dir
}

#[test]
fn storing_the_same_bytes_twice_leaves_one_object() {
    let run = run_dir();
    let store = ObjectStore::at(run.path());

    let first = store.put(b"the same bytes").expect("store the bytes");
    let second = store.put(b"the same bytes").expect("store them again");

    assert_eq!(first, second, "the same content is the same object");
    assert_eq!(first, sha256_hex(b"the same bytes"));
    let objects: Vec<String> = std::fs::read_dir(run.path().join("objects"))
        .expect("the objects directory")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into())
        .collect();
    assert_eq!(
        objects,
        vec![first.to_string()],
        "one object, named by its hash and nothing else"
    );
}

#[test]
fn an_object_is_named_by_its_hash_the_moment_it_appears() {
    let run = run_dir();
    let store = ObjectStore::at(run.path());

    let hash = store.put(b"whole or not at all").expect("store the bytes");

    let names: Vec<String> = std::fs::read_dir(run.path().join("objects"))
        .expect("the objects directory")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into())
        .collect();
    assert_eq!(
        names,
        vec![hash.to_string()],
        "written through a rename: `objects/` never holds a temporary name"
    );
    assert_eq!(
        store.get(&hash).expect("read it back"),
        b"whole or not at all"
    );
}

#[test]
fn reading_an_object_the_run_never_stored_says_it_is_missing() {
    let run = run_dir();
    let store = ObjectStore::at(run.path());
    let absent = sha256_hex(b"never stored");

    let error = store.get(&absent).expect_err("nothing to read");

    assert!(
        matches!(&error, ObjectError::Missing { hash } if *hash == absent),
        "{error:?}"
    );
}

#[test]
fn reading_an_object_whose_bytes_were_replaced_names_both_hashes() {
    let run = run_dir();
    let store = ObjectStore::at(run.path());
    let hash = store.put(b"the accepted bytes").expect("store the bytes");
    std::fs::write(run.path().join("objects").join(hash.as_str()), b"tampered").expect("replace");

    let error = store
        .get(&hash)
        .expect_err("the object no longer is itself");

    let found = sha256_hex(b"tampered");
    assert!(
        matches!(
            &error,
            ObjectError::Corrupt { hash: named, found: actual } if *named == hash && *actual == found
        ),
        "{error:?}"
    );
    let text = error.to_string();
    assert!(
        text.contains(hash.as_str()),
        "names the hash asked for: {text}"
    );
    assert!(text.contains(found.as_str()), "names what it found: {text}");
}

#[test]
fn a_producers_artifact_projects_under_its_node() {
    let run = run_dir();
    let store = ObjectStore::at(run.path());
    let hash = store.put(b"the report").expect("store the bytes");

    store
        .project(Some(&"review".into()), "report.md", &hash)
        .expect("write the view");

    assert_eq!(
        std::fs::read(
            run.path()
                .join("artifacts")
                .join("review")
                .join("report.md")
        )
        .expect("the view exists under the node"),
        b"the report"
    );
}

#[test]
fn what_the_run_acquires_without_a_producer_projects_at_the_root() {
    let run = run_dir();
    let store = ObjectStore::at(run.path());
    let hash = store.put(b"inherited").expect("store the bytes");

    store
        .project(None, "brief/plan.md", &hash)
        .expect("write the view");

    assert_eq!(
        std::fs::read(run.path().join("artifacts").join("brief").join("plan.md"))
            .expect("a nested name nests"),
        b"inherited"
    );
}

#[test]
fn projecting_an_object_the_run_never_stored_says_it_is_missing() {
    let run = run_dir();
    let store = ObjectStore::at(run.path());
    let absent = sha256_hex(b"never stored");

    let error = store
        .project(None, "plan.yaml", &absent)
        .expect_err("there are no bytes to project");

    assert!(
        matches!(&error, ObjectError::Missing { hash } if *hash == absent),
        "{error:?}"
    );
}
