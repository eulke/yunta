//! See [`super`]. A run followed that cannot be read beside the field followed: said with
//! why, as news of that run, while the field's runs move on, and read whole once it reads.

use yunta_api::{FieldReach, FollowedRun};
use yunta_render::page::RunUnread;

use super::super::World;
use super::{at, each_run_stands_as_its_log, followed_in, heads, took, Kept, ROUNDS};

/// A run followed whose manifest does not read beside a field followed: the following is
/// made and the field's runs move as their logs do, look after look; once the manifest
/// reads and the run's head moves, the run followed is read whole.
#[tokio::test]
async fn a_followed_run_that_cannot_be_read_leaves_the_field_moving() {
    let world = World::new();
    let (first, _) = world.born("first").await;
    let (broken, broken_dir) = world.born("broken").await;
    let manifest = yunta_engine::run_dir::manifest_path(&broken_dir);
    let good = std::fs::read(&manifest).unwrap();
    std::fs::write(&manifest, "not: a manifest\n").unwrap();
    let mut following = followed_in(&world, FieldReach::Machine, Some(&broken)).await;
    let mut kept = Kept::default();

    for _ in 0..ROUNDS {
        world.slept(&first);
        let awaited = heads(&world, &[&first]);
        while !at(&kept, &awaited) {
            took(&mut following, &mut kept).await;
        }
    }
    each_run_stands_as_its_log(&world, &kept);
    assert!(kept.whole.is_none(), "the run followed is not read");
    let unread = kept.whole_unreadable.as_ref().map(|unread| &unread.problem);
    assert_eq!(
        unread,
        Some(&RunUnread::Manifest {
            path: manifest.clone()
        }),
        "the run followed is said unreadable, with why"
    );

    std::fs::write(&manifest, good).unwrap();
    world.slept(&broken);
    while kept.whole.is_none() {
        took(&mut following, &mut kept).await;
    }
    let whole = kept.whole.as_ref().map(FollowedRun::run);
    assert_eq!(whole, Some(&broken), "read whole once it reads");
}
