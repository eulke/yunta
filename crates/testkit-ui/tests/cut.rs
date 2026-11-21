//! A cut is a log followed: the design's run and the small runs, each cut at one of its
//! moments and loaded into a fresh store, are read by the follower as replay reads the log
//! up to that moment; several runs loaded into one store are one field; a run's screen
//! stands as its target needs (`screens`), the field as the design draws it (`field`), and
//! each target at the moment its screen needs (`moments`); every target screen's cut names a
//! run that is built and a moment its log holds; and every place of the home a target opens
//! in holds what the target shows (`home`).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

#[path = "cut/field.rs"]
mod field;
#[path = "cut/home.rs"]
mod home;
#[path = "cut/moments.rs"]
mod moments;
#[path = "cut/screens.rs"]
mod screens;

use yunta_api::{Awaited, FieldReach, FollowNews, Followed, FollowedRun, Following, RunNews, Stop};
use yunta_core::events::StoredEvent;
use yunta_core::RunId;
use yunta_replay::RunState;
use yunta_testkit_ui::cut::{Built, Cut, Shown, Stands, TARGET_CUTS, WITH_FRAGUA};
use yunta_testkit_ui::designed;

/// How long a piece of news may take to come before the test says what it waited for: the
/// follower looks every tenth of a second and reads what a run asks on a task of its own.
const PATIENCE: Duration = Duration::from_secs(10);

/// The seq the design's run parks at on its plan's gate.
const PLANS_GATE: u64 = 78;

/// How many target screens the targets' tasks write.
const TARGETS: usize = 102;

/// The place the kept log names wherever a path of the recording's place stood.
const KEPT_PLACE: &str = "/designed";

/// The kept log of the design's run, one event a line, its paths naming `cut`'s place as
/// the cut names them.
fn kept_in(cut: &Cut) -> Vec<StoredEvent> {
    designed::log()
        .replace(KEPT_PLACE, &cut.root().display().to_string())
        .lines()
        .map(|line| serde_json::from_str(line).expect("an event of the kept log"))
        .collect()
}

/// The design's run, as `cut` holds it.
fn design() -> RunId {
    RunId::from(designed::RUN)
}

/// `followed`, followed in `cut`'s store as the application follows it.
async fn following(cut: &Cut, followed: Followed) -> Following {
    cut.workspace()
        .project()
        .expect("the cut's repository")
        .follow(followed)
        .await
        .expect("the cut, followed")
}

/// The next piece of news `following` hands, which comes within [`PATIENCE`].
async fn next(following: &mut Following, waited: &str) -> FollowNews {
    tokio::time::timeout(PATIENCE, following.next(&Stop::new()))
        .await
        .unwrap_or_else(|_| panic!("no news came while the test waited for {waited}"))
        .expect("the following reads on")
}

/// The run `run` read whole, as the following's first piece of news hands it.
async fn read_whole(cut: &Cut, run: &RunId) -> (FollowedRun, Vec<StoredEvent>) {
    let followed = Followed {
        field: None,
        run: Some(run.clone()),
    };
    let mut following = following(cut, followed).await;
    match next(&mut following, "the run read whole").await {
        FollowNews::Whole { run, events } => (*run, events),
        other => panic!("the first piece of news is the run read whole: {other:?}"),
    }
}

/// `events`' seqs, in order.
fn seqs(events: &[StoredEvent]) -> Vec<u64> {
    events.iter().map(|event| event.seq.get()).collect()
}

#[tokio::test]
async fn the_designs_run_cut_at_the_plans_gate_reads_as_its_log_up_to_there_and_asks_the_plan() {
    let cut = Cut::of(&[Shown::at(Built::Designed, PLANS_GATE)]).await;
    let followed = Followed {
        field: None,
        run: Some(design()),
    };
    let mut following = following(&cut, followed).await;
    let up_to_the_gate: Vec<StoredEvent> = kept_in(&cut)
        .into_iter()
        .take_while(|event| event.seq.get() <= PLANS_GATE)
        .collect();
    let FollowNews::Whole { run, events } = next(&mut following, "the run read whole").await else {
        panic!("the first piece of news is the run read whole");
    };
    assert_eq!(events, up_to_the_gate);
    assert_eq!(run.state, RunState::replay(&up_to_the_gate));
    assert_eq!(run.state.head().map(|seq| seq.get()), Some(PLANS_GATE));
    let asked = loop {
        match next(&mut following, "what the run asks").await {
            FollowNews::Run {
                news: RunNews::Awaited(Some((_, asked))),
                ..
            } => break asked,
            FollowNews::Run {
                news: RunNews::AwaitedUnread { why, .. },
                ..
            } => panic!("what the run asks could not be read: {why}"),
            _ => {}
        }
    };
    let Awaited::Decision(decision) = asked else {
        panic!("the run asks the plan's decision, not a round");
    };
    assert_eq!(
        decision.on.node().map(|node| node.as_str()),
        Some("approve-plan")
    );
}

#[tokio::test]
async fn at_every_tenth_seq_of_the_designs_run_the_follower_reads_the_replay_of_the_cut() {
    let length = designed::log().lines().count();
    for seq in (10..=length).step_by(10) {
        let cut = Cut::of(&[Shown::at(Built::Designed, seq as u64)]).await;
        let (run, events) = read_whole(&cut, &design()).await;
        let state = run.state;
        let kept = kept_in(&cut);
        let up_to: &[StoredEvent] = &kept[..seq];
        assert_eq!(seqs(&events), seqs(up_to), "the log cut at {seq}");
        assert_eq!(events, up_to, "the log cut at {seq}");
        assert_eq!(state, RunState::replay(up_to), "the run cut at {seq}");
        assert_eq!(state.head().map(|head| head.get()), Some(seq as u64));
    }
}

#[tokio::test]
async fn three_runs_loaded_at_their_moments_are_followed_as_one_field() {
    let created = Built::Created.build().await;
    let console = Built::ConsoleGate.build().await;
    let shown = [
        Shown::at(Built::Designed, PLANS_GATE),
        Shown::at(Built::Created, created.last()),
        Shown::at(Built::ConsoleGate, console.last()),
    ];
    let cut = Cut::of(&shown).await;
    let followed = Followed {
        field: Some(FieldReach::Here),
        run: None,
    };
    let mut following = following(&cut, followed).await;
    let FollowNews::Field(field) = next(&mut following, "the field read whole").await else {
        panic!("the first piece of news is the field read whole");
    };
    assert!(field.here, "the field is the cut's repository's");
    assert!(field.unreadable.is_empty(), "{:?}", field.unreadable);
    assert_eq!(field.elsewhere, 0);
    let heads: BTreeMap<&RunId, u64> = field
        .runs
        .iter()
        .map(|run| (&run.run, run.state.head().map_or(0, |seq| seq.get())))
        .collect();
    let expected: BTreeMap<&RunId, u64> = [
        (&cut.runs()[0], PLANS_GATE),
        (&cut.runs()[1], created.last()),
        (&cut.runs()[2], console.last()),
    ]
    .into_iter()
    .collect();
    assert_eq!(heads, expected);
}

#[tokio::test]
async fn every_target_cut_names_a_run_that_is_built_and_a_seq_its_log_holds() {
    let names: BTreeSet<&str> = TARGET_CUTS.iter().map(|target| target.name).collect();
    assert_eq!(names.len(), TARGET_CUTS.len(), "a target is named twice");
    assert_eq!(TARGET_CUTS.len(), TARGETS);
    let mut lasts: BTreeMap<Built, u64> = BTreeMap::new();
    let cuts = TARGET_CUTS.iter().filter_map(|target| match target.stands {
        Stands::Run(_) | Stands::Field(_) => Some((target.name, target.stands.runs())),
        Stands::Home(_) => None,
    });
    let home = (
        "the home's repository set up as the design's",
        WITH_FRAGUA.to_vec(),
    );
    for (name, shown) in cuts.chain([home]) {
        assert!(!shown.is_empty(), "`{name}` shows no run");
        for each in &shown {
            let last = match lasts.get(&each.built) {
                Some(last) => *last,
                None => {
                    let last = each.built.build().await.last();
                    lasts.insert(each.built, last);
                    last
                }
            };
            assert!(
                (1..=last).contains(&each.seq),
                "`{name}` cuts {:?} at {}, and its log ends at {last}",
                each.built,
                each.seq
            );
        }
    }
    let scenes = [
        Built::TwoTasks,
        Built::ModeSkips,
        Built::SentBack,
        Built::Refused,
        Built::Unread,
    ];
    for scene in scenes {
        assert!(lasts.contains_key(&scene), "no target cuts {scene:?}");
    }
}
