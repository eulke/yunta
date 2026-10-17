//! What a durable write promises: a replace that fails leaves its target
//! as it was and no staging file beside it, and however many callers race
//! to create one path, exactly one of them creates it.

use yunta_core::durable::{create_new, replace, Durability};

#[test]
fn a_failed_replace_leaves_the_target_and_no_staging_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("engine.json");
    std::fs::create_dir(&target).unwrap();
    assert!(replace(&target, b"{}", Durability::Atomic).is_err());
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("engine.json")]);
}

#[test]
fn sixteen_creators_of_one_path_yield_exactly_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("yunta.lock");
    let wins = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..16)
            .map(|_| scope.spawn(|| create_new(&target, b"owner", Durability::Atomic).unwrap()))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|won| *won)
            .count()
    });
    assert_eq!(wins, 1);
    assert_eq!(std::fs::read(&target).unwrap(), b"owner");
}
