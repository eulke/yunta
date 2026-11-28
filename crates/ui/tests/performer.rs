//! The performer, against a workspace a test makes in a temporary directory: what an
//! operation comes to arrives as an input under its ticket, and what failed below
//! Yunta comes back in its own words; what it is asked to follow, it follows in one
//! following, whose news come under no ticket, and a look of it that fails, its making
//! among them, is told and followed past. It reads nothing of its process: a person's
//! answers it records carry the channel its surface made it with.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use yunta_api::Workspace;
use yunta_api::{FieldReach, FollowNews, Followed, Repository, RunNews};
use yunta_core::events::{
    Channel, EventDraft, EventPayload, GateEvent, HostSuspendedPayload, RunEvent,
};
use yunta_core::{Answer, NodeId, PricingEntry, QuestionId, RunId, Seq};
use yunta_testkit::workflows::{ASKS, FINISHES, GATED, HANDS_ONE_QUESTION_OVER};
use yunta_testkit::Bench;
use yunta_testkit_core::FixedClock;
use yunta_testkit_ui::driving::{carried_out, drive};
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::{Input, Operation, Outcome, Performer, Request, State};

/// A mock fixture with no sessions: neither workflow opens one.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// What the performer hands back for the operation the opening state asks.
async fn opened_in(workspace: Workspace) -> Outcome {
    let (_, requests) = State::opened(now());
    let [Request::Perform(ticket, operation)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    assert_eq!(*operation, Operation::ReadRepository);
    let (inputs, mut received) = mpsc::unbounded_channel();
    let performer = Arc::new(Performer::new(workspace, Channel::Tty));
    let asked = Request::Perform(*ticket, operation.clone());
    assert!(performer.take(asked, &inputs).is_none());
    loop {
        match received.recv().await.expect("an outcome") {
            Input::Outcome(answered, outcome) => {
                assert_eq!(answered, *ticket, "under the ticket it was asked under");
                return outcome;
            }
            Input::Remark(..) => continue,
            other => panic!("{other:?}"),
        }
    }
}

/// The repository the application works in is answered as far as Yunta goes: none
/// outside one, never set up in a repository with no config of Yunta's, and set up
/// in one that has it.
#[tokio::test]
async fn the_repository_it_works_in_is_answered() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let answered = opened_in(workspace(&outside, root.path())).await;
    assert!(
        matches!(&answered, Outcome::Repository(read) if read.repository == Repository::Absent),
        "{answered:?}"
    );

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let answered = opened_in(workspace(&repo, root.path())).await;
    assert!(
        matches!(&answered, Outcome::Repository(read) if read.repository == Repository::NeverSetUp { dir: repo.clone() }),
        "{answered:?}"
    );

    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    std::fs::write(repo.join(".yunta/config.yaml"), "").unwrap();
    let answered = opened_in(workspace(&repo, root.path())).await;
    assert!(
        matches!(&answered, Outcome::Repository(read) if read.repository == Repository::SetUp { dir: repo.clone() }),
        "{answered:?}"
    );
}

/// The repository read says where the project keeps its runs and what it prices tokens
/// at, as its configuration says them, neither of which is followed.
#[tokio::test]
async fn the_repository_read_says_where_its_runs_are_kept_and_its_prices() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    yunta_testkit::init_repo(&repo);
    let kept = root.path().join("kept").join("runs.db");
    let config = format!(
        "storage:\n  path: {}\npricing:\n  claude-opus:\n    cost_per_1k_tokens: 0.015\n",
        kept.display()
    );
    std::fs::write(repo.join(".yunta/config.yaml"), config).unwrap();
    let answered = opened_in(workspace(&repo, root.path())).await;
    let Outcome::Repository(read) = answered else {
        panic!("{answered:?}");
    };
    assert_eq!(read.repository, Repository::SetUp { dir: repo.clone() });
    assert_eq!(read.storage, kept, "the store its configuration names");
    let priced = BTreeMap::from([(
        "claude-opus".to_string(),
        PricingEntry {
            cost_per_1k_tokens: 0.015,
        },
    )]);
    assert_eq!(read.prices, Some(priced));
}

#[tokio::test]
async fn what_failed_below_yunta_comes_back_in_its_own_words() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    yunta_testkit::init_repo(&repo);
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "runners: [not, a, mapping\n",
    )
    .unwrap();
    let answered = opened_in(workspace(&repo, root.path())).await;
    let Outcome::Failed(said) = answered else {
        panic!("{answered:?}");
    };
    assert!(said.contains("config.yaml"), "{said}");
}

/// A round answered by a performer its surface made on the MCP channel is on the run's
/// log under that channel: the performer records an answer under the channel it was
/// made with, never one of its own.
#[tokio::test]
async fn a_round_is_answered_on_the_channel_the_performer_was_made_with() {
    let asking = Bench::with_run_id("01M7PERF0RMASK1NG000000000");
    asking.run(ASKS, HANDS_ONE_QUESTION_OVER).await;
    let root = asking
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let performer = Arc::new(Performer::new(
        workspace(&asking.worktree, root),
        Channel::Mcp,
    ));
    // A ticket of the application's own: the performer hands back under what it is given.
    let (_, requests) = State::opened(now());
    let Some(Request::Perform(ticket, _)) = requests.into_iter().next() else {
        panic!("the application asks something as it opens");
    };
    let answer = Operation::Answer {
        run: asking.run_id.clone(),
        node: NodeId::from("ask"),
        answers: vec![Answer {
            id: QuestionId::from("summary"),
            value: "the greeting".to_string(),
        }],
    };
    carried_out(&performer, ticket, answer).await;
    let channels: Vec<Channel> = asking
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::QuestionsAnswered(answered))) => {
                Some(answered.channel)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        channels,
        [Channel::Mcp],
        "the round is answered once, on MCP"
    );
}

/// Driven outside a repository, the application comes to rest on the signpost, waiting
/// on nothing, and leaves at `q`.
#[tokio::test]
async fn the_signpost_is_driven_to_quiet() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let performer = std::sync::Arc::new(Performer::new(
        workspace(&outside, root.path()),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let strokes = [yunta_ui::prompt::Stroke::Insert('q')];
    let (state, surfaced) = drive(opened, asked, &performer, &strokes).await;
    assert!(state.waiting_on().is_empty());
    let screen = surco::surface::Markdown.screen(&state.screen(&surco::surface::Markdown.look()));
    assert!(screen.contains("not in a repository"), "{screen}");
    assert_eq!(surfaced, [Request::Leave]);
}

/// Asked to follow the field, the performer hands it back read whole, under no ticket;
/// asked then to follow the field and the parked run, it follows on in the one following
/// it keeps: the run read whole, and the field not read again before it, as a second
/// following would read it first. Dropped, it leaves nothing following.
#[tokio::test]
async fn one_following_follows_what_it_is_asked_to() {
    let finished = Bench::with_run_id("01M7PERF0RMF1N1SHED0000000");
    finished.run(FINISHES, NO_SESSIONS).await;
    let parked = finished.beside("01M7PERF0RMPARKED000000000");
    parked.run(GATED, NO_SESSIONS).await;
    let root = finished
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let workspace = workspace(&finished.worktree, root);
    let performer = Arc::new(Performer::new(workspace, Channel::Tty));
    let (inputs, mut received) = mpsc::unbounded_channel();

    let field = Followed {
        field: Some(FieldReach::Here),
        run: None,
    };
    assert_eq!(performer.take(Request::Follow(field), &inputs), None);
    let read = match received.recv().await.expect("what the following hands") {
        Input::FollowNews(FollowNews::Field(read)) => read,
        other => panic!("the field read whole comes first: {other:?}"),
    };
    let runs: BTreeSet<RunId> = read.runs.iter().map(|run| run.run().clone()).collect();
    let made = BTreeSet::from([finished.run_id.clone(), parked.run_id.clone()]);
    assert_eq!(runs, made);

    let field_and_run = Followed {
        field: Some(FieldReach::Here),
        run: Some(parked.run_id.clone()),
    };
    assert_eq!(
        performer.take(Request::Follow(field_and_run), &inputs),
        None
    );
    loop {
        match received.recv().await.expect("what the following hands") {
            Input::FollowNews(FollowNews::Whole { run, .. }) => {
                assert_eq!(*run.run(), parked.run_id);
                break;
            }
            Input::FollowNews(FollowNews::Field(_)) => panic!("the field is read once"),
            Input::FollowNews(_) => continue,
            other => panic!("{other:?}"),
        }
    }

    drop((performer, inputs));
    while let Some(input) = received.recv().await {
        assert!(matches!(input, Input::FollowNews(_)), "{input:?}");
    }
}

/// Appends to `bench`'s run an event that moves nothing but the run's clock, behind the
/// engine's back: the seq it took.
fn slept(bench: &Bench) -> Seq {
    let slept = HostSuspendedPayload::slept(Duration::from_secs(3));
    let slept = EventPayload::Run(RunEvent::HostSuspended(slept));
    let draft = EventDraft::new(bench.run_id.clone(), None, slept).unwrap();
    bench.storage.append(&draft, &FixedClock).unwrap()
}

/// A look at the log that fails whole, the log set aside behind the follower's back, is
/// handed with why, under no ticket; and the following goes on past it: the log put
/// back, the field's next news comes, with no second request to follow and no second
/// reading of the field.
#[tokio::test]
async fn a_look_that_fails_is_told_and_the_following_goes_on() {
    let parked = Bench::with_run_id("01M7PERF0RMBR0KEN000000000");
    parked.run(GATED, NO_SESSIONS).await;
    let store = parked.store_path();
    let root = store.parent().expect("a bench's log stands in its root");
    let performer = Arc::new(Performer::new(
        workspace(&parked.worktree, root),
        Channel::Tty,
    ));
    let (inputs, mut received) = mpsc::unbounded_channel();
    let field = Followed {
        field: Some(FieldReach::Here),
        run: None,
    };
    assert_eq!(performer.take(Request::Follow(field), &inputs), None);
    // The following holds the one sender left, so what comes ends where it ends.
    drop(inputs);
    let read = received.recv().await.expect("the field read whole");
    assert!(
        matches!(read, Input::FollowNews(FollowNews::Field(_))),
        "{read:?}"
    );

    yunta_testkit::set_log_aside(store);
    let why = loop {
        match received.recv().await.expect("a look that failed is told") {
            Input::LookFailed(why) => break why,
            Input::FollowNews(_) => continue,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(why, "failed to list runs");

    yunta_testkit::put_log_back(store);
    let seq = slept(&parked);
    loop {
        match received.recv().await.expect("the following goes on") {
            Input::FollowNews(FollowNews::Run {
                run,
                news: RunNews::Events(events),
            }) if run == parked.run_id => {
                assert_eq!(events.last().map(|event| event.seq), Some(seq));
                break;
            }
            Input::FollowNews(FollowNews::Field(_)) => panic!("the field is read once"),
            Input::LookFailed(_) | Input::FollowNews(_) => continue,
            other => panic!("{other:?}"),
        }
    }
}

/// A following that cannot be made, its log closed to every opener, is told as a look
/// that failed, and made again a pace later: the log opened again, the field comes read
/// whole, with no second request to follow.
#[tokio::test]
async fn a_following_that_cannot_be_made_is_told_and_made_again() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let store = root.path().join("yunta.db");
    drop(yunta_storage::Storage::open(&store).expect("the log"));
    let open_to = |mode| std::fs::set_permissions(&store, Permissions::from_mode(mode)).unwrap();
    open_to(0o000);
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (inputs, mut received) = mpsc::unbounded_channel();
    let field = Followed {
        field: Some(FieldReach::Here),
        run: None,
    };
    assert_eq!(performer.take(Request::Follow(field), &inputs), None);
    // The following holds the one sender left, so what comes ends where it ends.
    drop(inputs);
    let told = received.recv().await;
    let why = match told.expect("a following that cannot be made is told") {
        Input::LookFailed(why) => why,
        other => panic!("{other:?}"),
    };
    assert!(why.starts_with("failed to open the event log"), "{why}");

    open_to(0o644);
    loop {
        match received.recv().await.expect("the following is made again") {
            Input::LookFailed(_) => continue,
            Input::FollowNews(FollowNews::Field(read)) => {
                assert!(read.runs.is_empty(), "the log holds no run");
                break;
            }
            other => panic!("{other:?}"),
        }
    }
}
