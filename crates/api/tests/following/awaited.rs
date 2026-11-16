//! What a followed run waits on a person for: read once for each pause it waits at,
//! handed as none once it waits on none, and said unreadable, with why, when what it
//! shows fails today's shape. Over runs a bench parks.

use std::time::Duration;

use yunta_api::{
    AnswerablePause, Awaited, FollowNews, Followed, FollowedRun, Following, RunNews, Stop,
};
use yunta_core::events::{
    EventPayload, HumanChoice, PauseReason, RunEvent, RunPausedPayload, RunResumedPayload,
};
use yunta_core::ArtifactKind;
use yunta_testkit::Bench;
use yunta_testkit_core::FixedClock;

use super::documents::accepted_again;
use super::{next_of, seqs, whole, World, NOTHING_COMES};

/// A gate nobody answers: the run parks on a decision.
const GATED: &str = "\
name: gated
nodes:
  - id: ship
    kind: gate
    assignee: the release lead
    message: \"Ship what is on the branch?\"
";

/// Two gates, one after the other: the run parks on the first, and on the second once
/// the first is answered and the run woken.
const GATED_TWICE: &str = "\
name: gated-twice
nodes:
  - id: ship
    kind: gate
    assignee: the release lead
    message: \"Ship what is on the branch?\"
  - id: announce
    kind: gate
    depends_on: [ship]
    assignee: the release lead
    message: \"Announce what shipped?\"
";

/// No session: a gate runs none.
const NO_SESSION: &str = "dialect: claude-code\nsessions: []\n";

/// The planner, the node that specifies its tasks, and the gate that shows both.
pub(super) const SPECIFIED: &str = r#"
name: specified
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: spec
    kind: prompt
    runner: planner
    permissions: read-only
    depends_on: [plan]
    prompt: "Write the tests the plan's tasks are held to."
    artifacts:
      produces: [spec]
  - id: approve-plan
    kind: gate
    depends_on: [spec]
    assignee: lead
    message: "Plan and its tests registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows:
      - { node: plan, kind: tasks }
      - { node: spec, kind: spec }
"#;

/// A plan that writes a greeting, and the spec that holds it to its test.
pub(super) const SPECIFYING: &str = r#"
dialect: claude-code
sessions:
  - match_prompt_contains: "Hand over the tasks"
    steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Greet"
            description: "Writes the greeting the run is about."
            tasks:
              - { id: greet, title: "Write the greeting", description: "Writes greeting.txt, and a check of it.", scope: [greeting.txt, tests/greet.sh], changes: [{ at: greeting.txt, what: "the greeting", code: "Hello" }, { at: tests/greet.sh, what: "a check of the greeting", code: "test -s greeting.txt" }], outcome: "greeting.txt says hello", criteria: [{ cmd: "test \"$(cat greeting.txt 2>/dev/null)\" = Hello", proves: "the greeting says hello" }] }
    outcome: { type: completed, summary: planned }
  - match_prompt_contains: "Write the tests"
    steps:
      - type: run_tool
        tool: yunta_submit_spec
        arguments:
          document:
            specs:
              - { task: greet, files: [{ path: tests/greet.sh, content: "test \"$(cat greeting.txt 2>/dev/null)\" = Hello\n" }], tests: [{ cmd: "sh tests/greet.sh", proves: "the greeting says hello" }] }
    outcome: { type: completed, summary: specified }
"#;

/// Bytes a spec no longer reads as: its specs are no list.
const BROKEN_SPEC: &[u8] = b"specs: 7\n";

/// The next news of what the run `kept` holds waits on a person for, each news before it
/// folded into it.
async fn asked(following: &mut Following, kept: &mut FollowedRun) -> RunNews {
    loop {
        let news = next_of(following, kept).await;
        if asks(&news) {
            return news;
        }
    }
}

/// Whether `news` says what a run waits on a person for.
fn asks(news: &RunNews) -> bool {
    matches!(news, RunNews::Awaited(_) | RunNews::AwaitedUnread { .. })
}

/// Every news of the run `kept` holds that `following` hands within `wait`, each folded
/// into it.
pub(super) async fn within(
    following: &mut Following,
    kept: &mut FollowedRun,
    wait: Duration,
) -> Vec<RunNews> {
    let until = tokio::time::Instant::now() + wait;
    let mut came = Vec::new();
    while let Ok(news) = tokio::time::timeout_at(until, following.next(&Stop::new())).await {
        let FollowNews::Run { run, news } = news.expect("following reads the run") else {
            panic!("only news of the run held comes");
        };
        assert_eq!(run, kept.run, "news of the run held");
        kept.took(&news);
        came.push(news);
    }
    came
}

/// The node whose decision `news` hands, as a person is asked it.
fn decided_on(news: &RunNews) -> &str {
    decision_at(news).1
}

/// The pause `news` hands a decision at, and the node the decision is on.
fn decision_at(news: &RunNews) -> (&AnswerablePause, &str) {
    let RunNews::Awaited(Some((pause, Awaited::Decision(decision)))) = news else {
        panic!("a decision is handed, and came {news:?}");
    };
    let node = decision.on.node().expect("a node's decision").as_str();
    (pause, node)
}

/// The pause the run of `world` stands at, as its log recorded it.
fn standing_pause(world: &World) -> PauseReason {
    let logged = world.logged(&world.bench.run_id);
    logged
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Run(RunEvent::Paused(paused))) => Some(paused.pause.clone()),
            _ => None,
        })
        .expect("the run parked")
}

/// `option` recorded on the decision the run of `bench` waits on, through a connection
/// of its own, as another process records it.
async fn answered(bench: &Bench, option: &str) {
    let manifest = bench.manifest();
    let redactor = yunta_engine::run_redactor(&manifest, &*yunta_testkit::no_secrets()).unwrap();
    let choice = HumanChoice {
        option: option.into(),
        by: "mcp".into(),
        free_text: None,
    };
    let storage = bench.storage.async_handle();
    let dir = bench.run_dir();
    yunta_engine::resolve_gate(
        &manifest,
        &dir,
        &storage,
        &bench.run_id,
        &FixedClock,
        choice,
        &redactor,
    )
    .await
    .expect("the decision takes the option");
}

/// The spec the run of `world` holds, accepted again as bytes that fail today's shape,
/// under their own hash, as a binary that read specs another way could have accepted
/// them.
pub(super) fn broken_spec(world: &World) {
    accepted_again(world, ("spec", ArtifactKind::Spec), |_| {
        BROKEN_SPEC.to_vec()
    });
}

/// A run parked on a gate is read once, over many looks and an event that is no pause;
/// moving again, it waits on none; parked again, it is read again.
#[tokio::test]
async fn what_a_run_asks_is_read_once_for_each_pause() {
    let world = World::new();
    world.bench.run(GATED, NO_SESSION).await;
    let run = world.bench.run_id.clone();
    let pause = standing_pause(&world);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    let first = asked(&mut following, &mut kept).await;
    assert_eq!(decided_on(&first), "ship");
    let slept = world.slept(&run);
    let came = within(&mut following, &mut kept, NOTHING_COMES).await;
    let read_on = came
        .iter()
        .any(|news| matches!(news, RunNews::Events(events) if seqs(events) == [slept]));
    assert!(read_on, "the event that is no pause is read: {came:?}");
    assert!(
        !came.iter().any(asks),
        "the pause is read once, whatever the looks: {came:?}"
    );

    world.record(
        &run,
        EventPayload::Run(RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None))),
    );
    let moving = asked(&mut following, &mut kept).await;
    assert!(
        matches!(moving, RunNews::Awaited(None)),
        "a run that moves waits on none: {moving:?}"
    );
    world.record(
        &run,
        EventPayload::Run(RunEvent::Paused(RunPausedPayload { pause })),
    );
    let second = asked(&mut following, &mut kept).await;
    assert_eq!(decided_on(&second), "ship", "the second pause is read");
}

/// An answer another process records takes the decision off the run: what it waits on
/// is handed as none.
#[tokio::test]
async fn a_decision_answered_elsewhere_reads_as_answered() {
    let world = World::new();
    world.bench.run(GATED, NO_SESSION).await;
    let run = world.bench.run_id.clone();
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;
    let first = asked(&mut following, &mut kept).await;
    assert_eq!(decided_on(&first), "ship");

    answered(&world.bench, "approve").await;
    let news = asked(&mut following, &mut kept).await;
    assert!(
        matches!(news, RunNews::Awaited(None)),
        "the decision answered elsewhere waits on nobody: {news:?}"
    );
}

/// Each pause a person can answer comes with its key: the same pause, read again after
/// another run was followed, hands the key it handed; the next gate the run parks on,
/// once the first is answered, hands another.
#[tokio::test]
async fn what_a_run_asks_names_its_pause() {
    let world = World::new();
    world.bench.run(GATED_TWICE, NO_SESSION).await;
    let run = world.bench.run_id.clone();
    let (elsewhere, _) = world.born("elsewhere").await;
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;
    let first = asked(&mut following, &mut kept).await;
    let (ship, on) = decision_at(&first);
    assert_eq!(on, "ship");

    following.refollow(Followed {
        field: None,
        run: Some(elsewhere),
    });
    whole(&mut following).await;
    following.refollow(Followed {
        field: None,
        run: Some(run.clone()),
    });
    let (mut kept, _) = whole(&mut following).await;
    let again = asked(&mut following, &mut kept).await;
    assert_eq!(
        decision_at(&again),
        (ship, "ship"),
        "the pause read again hands the key it handed"
    );

    answered(&world.bench, "approve").await;
    let news = asked(&mut following, &mut kept).await;
    assert!(
        matches!(news, RunNews::Awaited(None)),
        "the first gate answered waits on nobody: {news:?}"
    );
    world.bench.wake().await;
    let second = asked(&mut following, &mut kept).await;
    let (announce, on) = decision_at(&second);
    assert_eq!(on, "announce");
    assert_ne!(announce, ship, "another pause hands another key");
}

/// A run let go and held again before any look read the other: what it asks is handed
/// again, by the key it was handed with. A refollow to the run held hands nothing new:
/// the next news of what it asks is the answer another process records.
#[tokio::test]
async fn a_run_held_again_is_asked_again() {
    let world = World::new();
    world.bench.run(GATED, NO_SESSION).await;
    let run = world.bench.run_id.clone();
    let (elsewhere, _) = world.born("elsewhere").await;
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;
    let first = asked(&mut following, &mut kept).await;
    let (ship, on) = decision_at(&first);
    assert_eq!(on, "ship");

    let held_again = Followed {
        field: None,
        run: Some(run.clone()),
    };
    following.refollow(Followed {
        field: None,
        run: Some(elsewhere),
    });
    following.refollow(held_again.clone());
    let (mut kept, _) = whole(&mut following).await;
    let again = asked(&mut following, &mut kept).await;
    assert_eq!(
        decision_at(&again),
        (ship, "ship"),
        "the run held again is asked again, by the key it was handed with"
    );

    following.refollow(held_again);
    answered(&world.bench, "approve").await;
    let news = asked(&mut following, &mut kept).await;
    assert!(
        matches!(news, RunNews::Awaited(None)),
        "a refollow to the run held hands nothing new: {news:?}"
    );
}

/// A spec the gate shows, whose bytes fail today's shape: what the run waits on is said
/// unreadable with why, and the following goes on reading the run.
#[tokio::test]
async fn a_shown_document_that_fails_its_shape_arrives_unreadable() {
    let world = World::new();
    world.bench.run(SPECIFIED, SPECIFYING).await;
    let run = world.bench.run_id.clone();
    broken_spec(&world);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    let news = asked(&mut following, &mut kept).await;
    let RunNews::AwaitedUnread { why, .. } = news else {
        panic!("the spec that fails its shape is said unreadable, and came {news:?}");
    };
    assert!(
        why.contains("spec.yaml") && why.contains("`specs`"),
        "why names the spec and where it fails its shape: {why}"
    );

    let slept = world.slept(&run);
    let RunNews::Events(events) = next_of(&mut following, &mut kept).await else {
        panic!("the following goes on reading the run");
    };
    assert_eq!(seqs(&events), vec![slept]);
}
