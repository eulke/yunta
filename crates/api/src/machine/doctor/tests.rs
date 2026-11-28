use yunta_render::page::doctor::CheckoutsKeep;

use yunta_core::{AdapterId, ConfigLayer, Workflow};

use super::{keep_of, served};

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

/// A workflow called `name` whose one node takes a session on the runner `runner`.
fn on_runner(name: &str, runner: &str) -> (String, Workflow) {
    let yaml = format!(
        "name: {name}\nnodes:\n  - {{ id: work, kind: prompt, prompt: \"do it\", runner: {runner} }}\n"
    );
    let workflow = yunta_core::workflow::read::read(&yaml, std::path::Path::new("w.yaml"))
        .expect("a workflow");
    (name.to_string(), workflow)
}

/// An adapter serves the workflows with a node whose runner names it as a candidate, its
/// first or a later one, and no other.
#[test]
fn an_adapter_serves_the_workflows_whose_runners_name_it() {
    let config = ConfigLayer::read(
        "runners:\n  planner:\n    - adapter: claude-code\n  reviewer:\n    - adapter: claude-code\n    - adapter: codex\n",
    )
    .expect("a config");
    let catalog = [
        on_runner("plan", "planner"),
        on_runner("review", "reviewer"),
    ];
    let codex = AdapterId::from_static("codex");
    assert_eq!(served(&codex, &config, &catalog), ["review"]);
    let claude = AdapterId::from_static("claude-code");
    assert_eq!(served(&claude, &config, &catalog), ["plan", "review"]);
}
