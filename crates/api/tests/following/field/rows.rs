//! A run's row in the field, as a following keeps it and as a listing reads it: made one
//! way, from the run as the follower reads it, so the two agree at one instant.

use std::collections::BTreeMap;

use yunta_api::{FieldReach, FollowNews, RunNews};
use yunta_core::events::{
    EventPayload, PauseReason, RunEvent, RunPausedPayload, RunResumedPayload,
};
use yunta_core::RunId;
use yunta_render::page::{FieldGroup, FieldRow};
use yunta_replay::registry::Engine;

use super::super::{driven_by_this_process, left_by_a_dead_engine, World, STALL_OWED};
use super::{followed_in, heads, listed, paused_and_resumed, read_on_agreeing, took, Kept, ROUNDS};

/// Each run `kept` holds, in the field and read whole, rows at the world's instant as the
/// field `reach` asks for reads now: the same runs, each framed alike with the same menu,
/// and the same runs unread and elsewhere.
async fn rows_agree(world: &World, kept: &Kept, reach: FieldReach) {
    let now = world.clock().now();
    let listed = listed(world, reach).await;
    let rows: BTreeMap<&RunId, &FieldRow> = listed
        .rows
        .iter()
        .map(|row| (&row.frame.run_id, row))
        .collect();
    let (listed_runs, kept_runs): (Vec<&RunId>, Vec<&RunId>) =
        (rows.keys().copied().collect(), kept.field.keys().collect());
    assert_eq!(listed_runs, kept_runs, "the same runs");
    for followed in kept.field.values().chain(&kept.whole) {
        let (row, listed) = (followed.row(now), rows[followed.run()]);
        assert_eq!(row.frame, listed.frame, "{} framed alike", followed.run());
        assert_eq!(
            row.menu,
            listed.menu,
            "{} with the same menu",
            followed.run()
        );
    }
    let unread: Vec<&RunId> = listed.unreadable.iter().map(|run| &run.run_id).collect();
    let kept_unread: Vec<&RunId> = kept.unreadable.keys().collect();
    assert_eq!(unread, kept_unread, "the same runs unread");
    assert_eq!(listed.elsewhere, kept.elsewhere, "as many elsewhere");
}

/// A run paused, as a person cancels it.
fn paused() -> EventPayload {
    let pause = PauseReason::Cancelled;
    EventPayload::Run(RunEvent::Paused(RunPausedPayload { pause }))
}

/// A run resumed.
fn resumed() -> EventPayload {
    EventPayload::Run(RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None)))
}

/// Followed in the field and as the run read whole, a run's row equals the one a listing
/// reads at the same instant: on the first look, and look after look as runs move, stop,
/// resume and join.
#[tokio::test]
async fn a_runs_row_and_its_whole_reading_agree_at_every_look() {
    let world = World::new();
    let (first, dir) = world.born("first").await;
    let _engine = driven_by_this_process(&dir);
    let (second, _) = world.born("second").await;
    let (third, _) = world.born("third").await;
    world.record(&third, paused());
    let mut following = followed_in(&world, FieldReach::Here, Some(&first)).await;
    let mut kept = Kept::default();
    let mut runs = vec![first.clone(), second.clone(), third.clone()];
    for round in 0..=ROUNDS {
        let awaited: Vec<&RunId> = runs.iter().collect();
        read_on_agreeing(&world, &mut following, &mut kept, &awaited).await;
        rows_agree(&world, &kept, FieldReach::Here).await;
        world.slept(&first);
        paused_and_resumed(&world, &second);
        if round == 1 {
            world.record(&third, resumed());
        }
        if round == 2 {
            let (joined, _) = world.born("joined").await;
            runs.push(joined);
        }
    }
}

/// An engine killed under the field commits nothing: its run's row reads stalled once the
/// following hands what its registry says, as a listing reads it then.
#[tokio::test]
async fn an_engine_killed_reads_stalled_in_its_row_with_nothing_committed() {
    let world = World::new();
    let (mut engines, mut dirs) = (Vec::new(), Vec::new());
    for run in ["first", "killed", "third"] {
        let (_, dir) = world.born(run).await;
        engines.push(driven_by_this_process(&dir));
        dirs.push(dir);
    }
    let killed = RunId::from("killed");
    let mut following = followed_in(&world, FieldReach::Machine, None).await;
    let mut kept = Kept::default();
    took(&mut following, &mut kept).await;
    let now = world.clock().now();
    assert_eq!(kept.field[&killed].row(now).group(), FieldGroup::Running);
    rows_agree(&world, &kept, FieldReach::Machine).await;

    let logged = heads(&world, &[&killed]);
    left_by_a_dead_engine(&dirs[1]);
    let news = tokio::time::timeout(STALL_OWED, took(&mut following, &mut kept))
        .await
        .expect("the gone engine is told within two seconds");
    let FollowNews::Run { run, news } = news else {
        panic!("news of a run of the field, and came {news:?}");
    };
    assert_eq!(run, killed);
    assert!(matches!(news, RunNews::Engine(Engine::Dead)), "{news:?}");
    assert_eq!(heads(&world, &[&killed]), logged, "nothing committed");
    assert_eq!(kept.field[&killed].row(now).group(), FieldGroup::Stalled);
    rows_agree(&world, &kept, FieldReach::Machine).await;
}

/// A run parked by the engine that drove it, then resumed by another: its row stands
/// stopped once the pause is read, and running again past the resume, as a listing reads
/// it each time.
#[tokio::test]
async fn a_run_parked_then_resumed_from_another_process_stands_in_its_new_group() {
    let world = World::new();
    let (run, dir) = world.born("parked").await;
    let (other, _) = world.born("other").await;
    let engine = driven_by_this_process(&dir);
    let mut following = followed_in(&world, FieldReach::Machine, Some(&run)).await;
    let mut kept = Kept::default();
    let runs = [&run, &other];
    let group = |kept: &Kept| kept.field[&run].row(world.clock().now()).group();
    read_on_agreeing(&world, &mut following, &mut kept, &runs).await;
    assert_eq!(group(&kept), FieldGroup::Running);

    world.record(&run, paused());
    drop(engine);
    read_on_agreeing(&world, &mut following, &mut kept, &runs).await;
    assert_eq!(group(&kept), FieldGroup::Stopped);
    rows_agree(&world, &kept, FieldReach::Machine).await;

    let _resumed_by = driven_by_this_process(&dir);
    world.record(&run, resumed());
    world.slept(&run);
    world.slept(&other);
    read_on_agreeing(&world, &mut following, &mut kept, &runs).await;
    assert_eq!(group(&kept), FieldGroup::Running);
    rows_agree(&world, &kept, FieldReach::Machine).await;
}
