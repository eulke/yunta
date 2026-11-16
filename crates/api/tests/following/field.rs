//! The field followed beside the run read whole, through one reader: what the first
//! look hands of the field, what each look after it hands of each run, and how a run's
//! fault stays with that run. Over logs and registries a test writes.

use std::collections::BTreeMap;

use surco::words::Standing;
use yunta_api::{
    FieldReach, FieldReading, FollowNews, Followed, FollowedRun, Following, RunNews, Stop,
};
use yunta_core::events::{
    EventPayload, PauseReason, RunEvent, RunPausedPayload, RunResumedPayload,
};
use yunta_core::persisted::PersistedDoc;
use yunta_core::{RunId, Seq};
use yunta_render::page::{RunUnread, UnreadableRun};
use yunta_replay::registry::Engine;
use yunta_replay::RunState;
use yunta_testkit::{Bench, SourceLog};

use super::{
    driven_by_this_process, left_by_a_dead_engine, next, World, NOTHING_COMES, STALL_OWED, WORKFLOW,
};

#[path = "field/rows.rs"]
mod rows;

/// The field and the run read whole as an interface keeps them from what a following
/// hands: each run of the field at its head, those it cannot read, how many it leaves
/// to other repositories, and the run read whole.
#[derive(Default)]
struct Kept {
    field: BTreeMap<RunId, FollowedRun>,
    unreadable: BTreeMap<RunId, UnreadableRun>,
    elsewhere: usize,
    here: bool,
    whole: Option<FollowedRun>,
}

impl Kept {
    /// `news` folded in: a piece of news of a run reaches both its place in the field
    /// and the run read whole, when it is that run.
    fn took(&mut self, news: &FollowNews) {
        match news {
            FollowNews::Field(field) => {
                self.field = field
                    .runs
                    .iter()
                    .map(|run| (run.run.clone(), run.clone()))
                    .collect();
                self.unreadable = field
                    .unreadable
                    .iter()
                    .map(|run| (run.run_id.clone(), run.clone()))
                    .collect();
                self.elsewhere = field.elsewhere;
                self.here = field.here;
            }
            FollowNews::Joined(run) => {
                self.unreadable.remove(&run.run);
                self.field.insert(run.run.clone(), (**run).clone());
            }
            FollowNews::Unreadable(unread) => {
                self.field.remove(&unread.run_id);
                self.unreadable
                    .insert(unread.run_id.clone(), unread.clone());
            }
            FollowNews::Left(run) => {
                self.field.remove(run);
                self.unreadable.remove(run);
            }
            FollowNews::Elsewhere(elsewhere) => self.elsewhere = *elsewhere,
            FollowNews::Whole { run, .. } => self.whole = Some((**run).clone()),
            FollowNews::Run { run, news } => {
                if let Some(whole) = self.whole.as_mut().filter(|whole| whole.run == *run) {
                    whole.took(news);
                }
                if let (Some(kept), RunNews::Events(_) | RunNews::Engine(_)) =
                    (self.field.get_mut(run), news)
                {
                    kept.took(news);
                }
            }
        }
    }
}

/// What `field` reaches, and `run` read whole, followed from here.
async fn followed_in(world: &World, field: FieldReach, run: Option<&RunId>) -> Following {
    let followed = Followed {
        field: Some(field),
        run: run.cloned(),
    };
    world.project.follow(followed).await.unwrap()
}

/// The field `reach` asks for, as a listing reads it now.
async fn listed(world: &World, reach: FieldReach) -> FieldReading {
    world.project.field(reach, &Stop::new()).await.unwrap()
}

/// The next news of `following`, folded into `kept`.
async fn took(following: &mut Following, kept: &mut Kept) -> FollowNews {
    let news = next(following).await;
    kept.took(&news);
    news
}

/// Each run of the field `kept` holds, at its head, stands as its log replays up to that
/// head: its state, and the head itself, which a state's equality leaves out.
fn each_run_stands_as_its_log(world: &World, kept: &Kept) {
    for (run, followed) in &kept.field {
        let head = followed.state.head();
        let logged: Vec<_> = world
            .logged(run)
            .into_iter()
            .take_while(|event| Some(event.seq) <= head)
            .collect();
        let replayed = RunState::replay(&logged);
        assert_eq!(followed.state, replayed, "{run} as its log replays");
        assert_eq!(head, replayed.head(), "{run} at its head");
    }
}

/// The last seq of each of `runs`' logs.
fn heads(world: &World, runs: &[&RunId]) -> BTreeMap<RunId, Option<Seq>> {
    runs.iter()
        .map(|run| {
            (
                (*run).clone(),
                world.logged(run).last().map(|event| event.seq),
            )
        })
        .collect()
}

/// Whether the field `kept` holds stands at `heads`, each run of it at its log's head.
fn at(kept: &Kept, heads: &BTreeMap<RunId, Option<Seq>>) -> bool {
    heads.iter().all(|(run, head)| {
        kept.field
            .get(run)
            .is_some_and(|followed| followed.state.head() == *head)
    })
}

/// A run paused, as a person cancels it, then resumed: two events that change the
/// run's state, where a sleep moves only its clock.
fn paused_and_resumed(world: &World, run: &RunId) {
    world.record(
        run,
        EventPayload::Run(RunEvent::Paused(RunPausedPayload {
            pause: PauseReason::Cancelled,
        })),
    );
    world.record(
        run,
        EventPayload::Run(RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None))),
    );
}

/// How many times a test moves the runs it follows, each read on before the next.
const ROUNDS: usize = 4;

/// Run `run`, born on `world`'s log with a manifest frozen in another repository, which
/// a field of this repository leaves elsewhere.
async fn born_elsewhere(world: &World, other: &Bench, run: &str) -> RunId {
    let run = RunId::from(run);
    let dir = world.bench.runs_root.join(run.as_str());
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = other.manifest_for(WORKFLOW, "").await;
    let bytes = PersistedDoc::of(manifest).write().unwrap();
    std::fs::write(yunta_engine::run_dir::manifest_path(&dir), bytes).unwrap();
    SourceLog::open(&world.bench.storage, &run, world.clock());
    run
}

/// The first piece of news holds the whole field, each run of it as its log replays;
/// then, look after look, each run of the field stands as its log replays up to its
/// head, a run born after joins, and one born in another repository is counted there.
#[tokio::test]
async fn followed_in_the_field_each_run_stands_as_its_log_replays_at_every_look() {
    let world = World::new();
    let other = Bench::with_run_id("other");
    let (first, _) = world.born("first").await;
    let (second, _) = world.born("second").await;
    let (third, _) = world.born("third").await;
    world.slept(&second);
    born_elsewhere(&world, &other, "abroad").await;
    let mut following = followed_in(&world, FieldReach::Here, None).await;
    let mut kept = Kept::default();

    let news = took(&mut following, &mut kept).await;
    assert!(
        matches!(news, FollowNews::Field(_)),
        "the field comes first: {news:?}"
    );
    assert_eq!(
        kept.field.keys().collect::<Vec<_>>(),
        vec![&first, &second, &third],
        "every run of this repository"
    );
    assert!(kept.here, "the field is this repository's");
    assert_eq!(kept.elsewhere, 1, "the run of another repository");
    assert!(kept.unreadable.is_empty());
    each_run_stands_as_its_log(&world, &kept);

    let mut runs = vec![first.clone(), second.clone(), third.clone()];
    for round in 0..ROUNDS {
        world.slept(&first);
        paused_and_resumed(&world, &third);
        if round == 1 {
            let (joined, _) = world.born("joined").await;
            world.slept(&joined);
            runs.push(joined);
            born_elsewhere(&world, &other, "abroad-later").await;
        }
        let awaited = heads(&world, &runs.iter().collect::<Vec<_>>());
        while !at(&kept, &awaited) {
            took(&mut following, &mut kept).await;
            each_run_stands_as_its_log(&world, &kept);
        }
    }
    assert_eq!(kept.field.len(), 4, "the run born after joined");
    while kept.elsewhere != 2 {
        took(&mut following, &mut kept).await;
    }
}

/// The run read whole and its place in the field, as `kept` holds both: one state at one
/// head.
fn agree(kept: &Kept) {
    let Some(whole) = &kept.whole else {
        return;
    };
    let Some(placed) = kept.field.get(&whole.run) else {
        return;
    };
    assert_eq!(whole.state, placed.state, "{} one state", whole.run);
    assert_eq!(
        whole.state.head(),
        placed.state.head(),
        "{} one head",
        whole.run
    );
}

/// News of `following` folded into `kept` until each of `runs` stands at its log's head
/// in the field and as the run read whole, the two agreeing at each piece.
async fn read_on_agreeing(
    world: &World,
    following: &mut Following,
    kept: &mut Kept,
    runs: &[&RunId],
) {
    let awaited = heads(world, runs);
    let held_at = |kept: &Kept| {
        kept.whole
            .as_ref()
            .is_some_and(|whole| awaited.get(&whole.run) == Some(&whole.state.head()))
    };
    while !(at(kept, &awaited) && held_at(kept)) {
        took(following, kept).await;
        agree(kept);
    }
}

/// A run of the field read whole stands at the head the field holds it at, though its
/// log moved past it between two looks; each piece of news after reaches both.
#[tokio::test]
async fn a_run_of_the_field_read_whole_never_disagrees_with_its_place_in_it() {
    let world = World::new();
    let (first, _) = world.born("first").await;
    let (second, _) = world.born("second").await;
    let mut following = followed_in(&world, FieldReach::Machine, Some(&first)).await;
    let mut kept = Kept::default();
    let runs = [&first, &second];
    read_on_agreeing(&world, &mut following, &mut kept, &runs).await;

    paused_and_resumed(&world, &second);
    following.refollow(Followed {
        field: Some(FieldReach::Machine),
        run: Some(second.clone()),
    });
    read_on_agreeing(&world, &mut following, &mut kept, &runs).await;
    assert_eq!(kept.whole.as_ref().map(|whole| &whole.run), Some(&second));
    for _ in 0..ROUNDS {
        paused_and_resumed(&world, &second);
        world.slept(&first);
        read_on_agreeing(&world, &mut following, &mut kept, &runs).await;
    }
}

/// A field read again at another reach beside the run read whole holds that run where
/// the run read whole stands, though its log moved past it between two looks; each
/// piece of news after reaches both.
#[tokio::test]
async fn a_field_read_again_never_disagrees_with_the_run_read_whole() {
    let world = World::new();
    let (first, _) = world.born("first").await;
    let (second, _) = world.born("second").await;
    let mut following = followed_in(&world, FieldReach::Machine, Some(&second)).await;
    let mut kept = Kept::default();
    let runs = [&first, &second];
    read_on_agreeing(&world, &mut following, &mut kept, &runs).await;

    paused_and_resumed(&world, &second);
    following.refollow(Followed {
        field: Some(FieldReach::Here),
        run: Some(second.clone()),
    });
    read_on_agreeing(&world, &mut following, &mut kept, &runs).await;
    for _ in 0..ROUNDS {
        paused_and_resumed(&world, &second);
        world.slept(&first);
        read_on_agreeing(&world, &mut following, &mut kept, &runs).await;
    }
}

/// An engine killed under the field commits nothing: its run's registry tells of it,
/// handed as that run's news, and no other run of the field moves.
#[tokio::test]
async fn an_engine_killed_in_the_field_is_handed_as_its_runs_engine_within_two_seconds() {
    let world = World::new();
    let mut engines = Vec::new();
    let mut runs = Vec::new();
    for run in ["first", "killed", "third"] {
        let (run, dir) = world.born(run).await;
        engines.push(driven_by_this_process(&dir));
        runs.push((run, dir));
    }
    let mut following = followed_in(&world, FieldReach::Machine, None).await;
    let mut kept = Kept::default();
    took(&mut following, &mut kept).await;
    let stop = Stop::new();

    let (killed, dir) = &runs[1];
    left_by_a_dead_engine(dir);
    let news = tokio::time::timeout(STALL_OWED, following.next(&stop))
        .await
        .expect("the gone engine is told within two seconds")
        .expect("following reads the field");
    kept.took(&news);
    let FollowNews::Run { run, news } = news else {
        panic!("news of a run of the field, and came {news:?}");
    };
    assert_eq!(&run, killed);
    assert!(matches!(news, RunNews::Engine(Engine::Dead)), "{news:?}");
    let frame = kept.field[killed].frame(world.clock().now());
    assert_eq!(frame.standing, Standing::Stalled);
    let after = tokio::time::timeout(NOTHING_COMES, following.next(&stop)).await;
    assert!(after.is_err(), "nothing else moved, and came {after:?}");
}

/// A run whose manifest does not read is said with why, and the rest are read on; a
/// run born with no directory is said too; once the manifest reads and its head moves,
/// it is read again whole and joins.
#[tokio::test]
async fn a_run_that_cannot_be_read_is_said_with_why_and_the_rest_go_on() {
    let world = World::new();
    let (first, _) = world.born("first").await;
    let (broken, broken_dir) = world.born("broken").await;
    let manifest = yunta_engine::run_dir::manifest_path(&broken_dir);
    let good = std::fs::read(&manifest).unwrap();
    std::fs::write(&manifest, "not: a manifest\n").unwrap();
    let (third, _) = world.born("third").await;
    let mut following = followed_in(&world, FieldReach::Machine, None).await;
    let mut kept = Kept::default();

    took(&mut following, &mut kept).await;
    assert_eq!(kept.field.keys().collect::<Vec<_>>(), vec![&first, &third]);
    let unread = &kept.unreadable[&broken].problem;
    assert_eq!(
        unread,
        &RunUnread::Manifest {
            path: manifest.clone()
        }
    );

    let homeless = RunId::from("homeless");
    SourceLog::open(&world.bench.storage, &homeless, world.clock());
    paused_and_resumed(&world, &first);
    world.slept(&third);
    let awaited = heads(&world, &[&first, &third]);
    while !(at(&kept, &awaited) && kept.unreadable.contains_key(&homeless)) {
        took(&mut following, &mut kept).await;
    }
    each_run_stands_as_its_log(&world, &kept);
    assert_eq!(
        kept.unreadable[&homeless].problem,
        RunUnread::NoDirectory {
            runs_root: world.bench.runs_root.clone()
        }
    );

    std::fs::write(&manifest, good).unwrap();
    world.slept(&broken);
    let awaited = heads(&world, &[&broken]);
    while !at(&kept, &awaited) {
        took(&mut following, &mut kept).await;
    }
    assert!(!kept.unreadable.contains_key(&broken), "read again whole");
    each_run_stands_as_its_log(&world, &kept);
}

/// A run whose log fails a read, as an event changed behind the store's back leaves it, is
/// said with why, broken before the field is read whole or past the head it was held at;
/// the rest are read on, and the field a listing reads says the same.
#[tokio::test]
async fn a_run_whose_log_fails_a_read_is_said_with_why_and_the_rest_go_on() {
    let world = World::new();
    let (first, _) = world.born("first").await;
    let (broken, _) = world.born("broken").await;
    let (later, _) = world.born("later").await;
    let storage = &world.bench.storage;
    let seq = world.slept(&broken);
    storage.corrupt_event_payload(&broken, seq).unwrap();
    let mut following = followed_in(&world, FieldReach::Machine, None).await;
    let mut kept = Kept::default();

    took(&mut following, &mut kept).await;
    assert_eq!(kept.field.keys().collect::<Vec<_>>(), vec![&first, &later]);
    let unread = &kept.unreadable[&broken].problem;
    assert!(matches!(unread, RunUnread::EventLog(_)), "{unread:?}");

    let seq = world.slept(&later);
    storage.corrupt_event_payload(&later, seq).unwrap();
    for _ in 0..ROUNDS {
        world.slept(&first);
        let awaited = heads(&world, &[&first]);
        while !(at(&kept, &awaited) && kept.unreadable.contains_key(&later)) {
            took(&mut following, &mut kept).await;
        }
    }
    assert_eq!(kept.field.keys().collect::<Vec<_>>(), vec![&first]);
    let unread = &kept.unreadable[&later].problem;
    assert!(matches!(unread, RunUnread::EventLog(_)), "{unread:?}");
    each_run_stands_as_its_log(&world, &kept);

    let listed = listed(&world, FieldReach::Machine).await.unreadable;
    let said = |run: &UnreadableRun| (run.run_id.clone(), run.problem.clone());
    let kept_said: Vec<_> = kept.unreadable.values().map(said).collect();
    assert_eq!(listed.iter().map(said).collect::<Vec<_>>(), kept_said);
}

/// A run of the field parked by the engine that drove it, then resumed by another: the
/// pause is handed, and the field reads on past the resume.
#[tokio::test]
async fn a_run_of_the_field_parked_then_resumed_from_another_process_is_read_on() {
    let world = World::new();
    let (run, dir) = world.born("parked").await;
    let (other, _) = world.born("other").await;
    let engine = driven_by_this_process(&dir);
    let mut following = followed_in(&world, FieldReach::Machine, None).await;
    let mut kept = Kept::default();
    took(&mut following, &mut kept).await;

    world.record(
        &run,
        EventPayload::Run(RunEvent::Paused(RunPausedPayload {
            pause: PauseReason::Cancelled,
        })),
    );
    drop(engine);
    let awaited = heads(&world, &[&run]);
    while !at(&kept, &awaited) {
        took(&mut following, &mut kept).await;
    }
    assert!(
        !kept.field[&run].frame(world.clock().now()).moves(),
        "parked"
    );

    let _resumed_by = driven_by_this_process(&dir);
    world.record(
        &run,
        EventPayload::Run(RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None))),
    );
    world.slept(&run);
    world.slept(&other);
    let awaited = heads(&world, &[&run, &other]);
    while !at(&kept, &awaited) {
        took(&mut following, &mut kept).await;
    }
    each_run_stands_as_its_log(&world, &kept);
    assert!(
        kept.field[&run].frame(world.clock().now()).moves(),
        "moves again"
    );
}
