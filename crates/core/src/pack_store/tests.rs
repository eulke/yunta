//! Where each store keeps its packs, its lock and its hold, and which pack a
//! path is in.

use super::*;

fn pack(reference: &str) -> PackRef {
    reference.parse().unwrap()
}

fn state_root(root: &str) -> Env {
    Env {
        yunta_home: Some(PathBuf::from(root)),
        ..Env::default()
    }
}

#[test]
fn a_project_keeps_its_packs_lock_and_hold_under_its_own_yunta_directory() {
    let store = PackStore::of_project(Path::new("/repo"));
    assert_eq!(store.scope(), PackScope::Project);
    assert_eq!(store.packs_dir(), Path::new("/repo/.yunta/packs"));
    assert_eq!(
        store.pack_dir(&pack("acme/review-pack")),
        Path::new("/repo/.yunta/packs/acme/review-pack")
    );
    assert_eq!(
        store.publisher_dir(&"acme".parse().unwrap()),
        Path::new("/repo/.yunta/packs/acme")
    );
    assert_eq!(store.lock_file(), Path::new("/repo/.yunta/yunta.lock"));
    assert_eq!(store.hold_file(), Path::new("/repo/.yunta/yunta.lock.held"));
}

#[test]
fn a_person_keeps_their_packs_lock_and_hold_under_the_state_root() {
    let store = PackStore::of_user(&state_root("/state")).unwrap();
    assert_eq!(store.scope(), PackScope::User);
    assert_eq!(
        store.pack_dir(&pack("acme/review-pack")),
        Path::new("/state/packs/acme/review-pack")
    );
    assert_eq!(store.lock_file(), Path::new("/state/yunta.lock"));
    assert_eq!(store.hold_file(), Path::new("/state/yunta.lock.held"));
}

#[test]
fn with_no_state_root_override_the_user_s_store_is_under_the_home_directory() {
    let env = Env {
        home: Some(PathBuf::from("/home/person")),
        ..Env::default()
    };
    let store = PackStore::of_user(&env).unwrap();
    assert_eq!(
        store.pack_dir(&pack("acme/review-pack")),
        Path::new("/home/person/.yunta/packs/acme/review-pack")
    );
}

#[test]
fn an_environment_that_names_no_state_root_has_no_user_store() {
    assert_eq!(PackStore::of_user(&Env::default()), None);
    let stores = PackStores::of(Path::new("/repo"), Some(&Env::default()));
    assert_eq!(stores.user(), None);
}

#[test]
fn no_environment_reads_only_the_project_s_store() {
    let stores = PackStores::of(Path::new("/repo"), None);
    let read: Vec<&PackStore> = stores.in_order().collect();
    assert_eq!(read, [&PackStore::of_project(Path::new("/repo"))]);
}

#[test]
fn a_project_reads_its_own_store_before_the_user_s() {
    let env = state_root("/state");
    let stores = PackStores::of(Path::new("/repo"), Some(&env));
    let scopes: Vec<PackScope> = stores.in_order().map(PackStore::scope).collect();
    assert_eq!(scopes, [PackScope::Project, PackScope::User]);
    assert_eq!(
        stores.project().packs_dir(),
        Path::new("/repo/.yunta/packs")
    );
    assert_eq!(
        stores.user().map(PackStore::packs_dir),
        Some(PathBuf::from("/state/packs"))
    );
}

#[test]
fn a_path_inside_a_pack_s_directory_is_held_by_that_pack() {
    let store = PackStore::of_user(&state_root("/state")).unwrap();
    let path = Path::new("/state/packs/acme/review-pack/workflows/review.yaml");
    assert_eq!(store.pack_holding(path), Some(pack("acme/review-pack")));
    let dir = Path::new("/state/packs/acme/review-pack");
    assert_eq!(store.pack_holding(dir), Some(pack("acme/review-pack")));
}

#[test]
fn a_path_outside_any_pack_of_the_store_is_held_by_none() {
    let store = PackStore::of_project(Path::new("/repo"));
    for path in [
        "/repo/.yunta/workflows/review.yaml",
        "/repo/.yunta/packs/acme",
        "/repo/.yunta/packs/acme/../review-pack/review.yaml",
        "/elsewhere/.yunta/packs/acme/review-pack/review.yaml",
    ] {
        assert_eq!(store.pack_holding(Path::new(path)), None, "{path}");
    }
}
