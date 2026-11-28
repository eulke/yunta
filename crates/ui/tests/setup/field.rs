//! See [`super`]. A setup follows the field of its repository's runs, as the first request
//! does: its tally and its attention row say what those runs need, `a` going where the row
//! points while no line is typed, and its sentence opens on what the field holds, once the
//! field is read. Written, it gives way to that field once a stage stands over it, where the
//! field holds a run.

use std::sync::Arc;

use surco::components::{Answer, Tally};
use surco::surface::Markdown;
use surco::words::Standing;
use yunta_api::{FollowNews, FollowedField};
use yunta_core::events::Channel;
use yunta_core::RunId;
use yunta_testkit::reference_pack;
use yunta_testkit::workflows::GATED;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace_reaching};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Performer, Request, Stage, State};

use super::{draft, found_by_hand, main_region, never_set_up, on_the_setup, out_of_reach, NOWHERE};

/// A mock fixture with no sessions: the run a test parks opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The run of a repository whose checkout holds no setup, parked on its gate.
const RAN: RunId = RunId::from_static("01M7SETVPRANBEF0REWA1T1NG0");

/// What a setup says over its rows in a repository whose field holds no run: the design's
/// sentence.
const NEVER_USED: &str =
    "This repository has never used yunta. Here is what it found. Nothing is written until you say so.";

/// What a setup says over its rows in a repository whose field holds a run.
const NO_SETUP_HERE: &str =
    "This checkout holds no yunta setup. Here is what it found. Nothing is written until you say so.";

/// The setup opened in a checkout that holds no setup, of a repository whose store holds a
/// run of it waiting on the person: the field read, its sentence says this checkout holds no
/// setup, its tally counts the run and its attention row names it with `a`, no line being
/// typed, which goes to the run's screen in the setup's place.
#[tokio::test]
async fn a_setup_of_a_repository_that_ran_says_so_and_names_its_run_waiting() {
    let world = Bench::with_run_id(RAN.as_str());
    world.run(GATED, NO_SESSIONS).await;
    let root = world
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    assert!(
        !world.worktree.join(".yunta/config.yaml").exists(),
        "the bench's checkout holds no setup"
    );
    let nowhere = reference_pack::out_of_reach(&root.join(NOWHERE));
    let performer = Arc::new(Performer::new(
        workspace_reaching(&world.worktree, root, Vec::new(), nowhere),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let (state, surfaced) = drive(opened, asked, &performer, &[]).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let screen = state.screen(&Markdown.look());
    let drawn = Markdown.screen(&screen);
    assert!(drawn.contains(NO_SETUP_HERE), "{drawn}");
    assert!(!drawn.contains(NEVER_USED), "{drawn}");
    let counted: &[_] = match &screen.tally {
        Tally::Counted(counts) => counts,
        Tally::Uncounted => &[],
    };
    let counted = counted.iter().map(|count| (count.standing, count.runs));
    assert!(
        counted.eq([(Standing::NeedsYou, 1)]),
        "the tally counts the run: {drawn}"
    );
    let attention = screen.attention.as_ref();
    let named = attention.and_then(|attention| attention.subject.as_deref());
    assert_eq!(named, Some(RAN.handle()), "{drawn}");
    assert!(
        attention.is_some_and(|attention| matches!(attention.answer, Answer::Key(_))),
        "`a` answers it where no line is typed: {drawn}"
    );
    let mut state = state;
    let asked = state.update(Input::Stroke(Stroke::Insert('a')));
    assert!(
        asked.iter().any(|asked| matches!(asked, Request::Follow(followed) if followed.run.as_ref() == Some(&RAN))),
        "`a` opens the run's screen in the setup's place: {asked:?}"
    );
}

/// The setup opened in a repository whose store holds no run of it says the design's
/// sentence, and that nothing needs the person.
#[tokio::test]
async fn a_setup_of_a_repository_that_never_ran_says_the_designs_sentence() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = never_set_up(root.path());
    let performer = Arc::new(Performer::new(
        out_of_reach(&repo, root.path()),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let (state, surfaced) = drive(opened, asked, &performer, &[]).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let screen = state.screen(&Markdown.look());
    let drawn = Markdown.screen(&screen);
    assert!(drawn.contains(NEVER_USED), "{drawn}");
    assert!(
        matches!(&screen.tally, Tally::Counted(counts) if counts.is_empty()),
        "nothing needs the person: {drawn}"
    );
    assert!(screen.attention.is_none(), "{drawn}");
}

/// What the setup found stands before the field is read, with no sentence over it: the
/// sentence comes with the field's first news, never to be taken back.
#[test]
fn the_sentence_waits_for_the_fields_first_news() {
    let (mut state, detecting) = on_the_setup();
    found_by_hand(&mut state, detecting, draft());
    let unread = main_region(&state);
    assert!(
        unread.contains("**name**"),
        "what was found stands: {unread}"
    );
    assert!(!unread.contains("Here is what it found"), "{unread}");
    let empty = FollowedField {
        runs: Vec::new(),
        unreadable: Vec::new(),
        elsewhere: 0,
        here: true,
    };
    let read = state.update(Input::FollowNews(FollowNews::Field(Box::new(empty))));
    assert!(read.is_empty(), "{read:?}");
    let region = main_region(&state);
    assert!(region.contains(NEVER_USED), "{region}");
}

/// A setup written in a checkout of a repository whose field holds a run gives way to that
/// field once a stage stands over it, as the first request does: going back from a new run
/// opened from what was written lands on the field, which nothing stands under.
#[tokio::test]
async fn a_setup_written_gives_way_to_the_field_holding_a_run() {
    let world = Bench::with_run_id(RAN.as_str());
    world.run(GATED, NO_SESSIONS).await;
    let root = world
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let nowhere = reference_pack::out_of_reach(&root.join(NOWHERE));
    let performer = Arc::new(Performer::new(
        workspace_reaching(&world.worktree, root, Vec::new(), nowhere),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let written_then_back = [Stroke::Enter, Stroke::Insert('n'), Stroke::Decline];
    let (state, _) = drive(opened, asked, &performer, &written_then_back).await;
    let trail = state.screen(&Markdown.look()).trail;
    assert!(
        matches!(state.stage(), Some(Stage::Field(_))),
        "back on the field: {trail:?}"
    );
    assert_eq!(state.stages_under().len(), 0, "{trail:?}");
}
