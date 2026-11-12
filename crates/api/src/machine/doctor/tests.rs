use yunta_render::page::doctor::CheckoutsKeep;

use super::keep_of;

/// What the checkouts keep, with `available` free of a disk that keeps
/// `keep_free`, and builds of `builds` within a `budget`.
fn kept(builds: u64, budget: u64, available: u64, keep_free: u64) -> yunta_engine::PoolReport {
    yunta_engine::PoolReport {
        checkouts: 2,
        builds,
        uncounted: 0,
        budget,
        available,
        keep_free,
    }
}

#[test]
fn checkouts_within_their_budget_and_over_the_free_part_are_within() {
    assert_eq!(keep_of(&kept(10, 100, 500, 50)), CheckoutsKeep::Within);
}

#[test]
fn builds_past_their_budget_are_over_it() {
    assert_eq!(
        keep_of(&kept(200, 100, 500, 50)),
        CheckoutsKeep::BuildsOverBudget
    );
}

/// A disk already under the part kept free stops a run, whatever the builds
/// take: it is what the check says first.
#[test]
fn a_disk_under_what_it_keeps_free_is_under_it_whatever_the_builds() {
    assert_eq!(
        keep_of(&kept(10, 100, 40, 50)),
        CheckoutsKeep::DiskUnderFree
    );
    assert_eq!(
        keep_of(&kept(200, 100, 40, 50)),
        CheckoutsKeep::DiskUnderFree
    );
}
