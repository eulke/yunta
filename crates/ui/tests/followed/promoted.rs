//! A run promoted before the application follows it: the state stands caught up only once
//! the successor already in the log is folded in its place.

use yunta_api::{FollowNews, RunNews, Stop};
use yunta_core::RunId;
use yunta_testkit::workflows::PROMOTABLE;
use yunta_testkit::{Bench, ScriptedInteraction};
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::now;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Request, State};

use super::support::{performer_in, stopped_beside};
use super::{in_view, Surface};

/// A run whose stage opens before it is promoted: the following made once its successor
/// stands in the log follows a run promoted before it is followed.
const PROMOTED: RunId = RunId::from_static("01M7F0110WEDPR0M0TEDBEF0RE");

/// A run promoted before it is followed, its successor already in the log: the state on
/// its stage stands caught up only once the successor is folded in its place.
#[tokio::test]
async fn a_run_promoted_before_it_is_followed_is_caught_up_on_its_successor() {
    let world = Bench::new().in_mode("quick");
    let config = world.worktree.join(".yunta/config.yaml");
    std::fs::create_dir_all(config.parent().expect("its .yunta")).expect("its .yunta");
    std::fs::write(&config, "defaults:\n  isolation: none\n").expect("its config");
    let stopped = stopped_beside(&world, &PROMOTED, PROMOTABLE).await;
    let (opened, asked) = State::opened(now());
    let (mut state, _) = drive(opened, asked, &performer_in(&world), &[Stroke::Enter]).await;
    let woken = stopped.with_cancel(Stop::new().work().clone());
    woken
        .wake_answering(&ScriptedInteraction::choose("promote"))
        .await;
    let successor = woken.successor("full").await;

    let asked = vec![Request::Follow(state.followed())];
    let news = Surface::over(&world)
        .caught_up(&mut state, asked, &world)
        .await;
    let handed_on = |input: &Input| {
        let onto = |news: &RunNews| matches!(news, RunNews::Successor { .. });
        matches!(input, Input::FollowNews(FollowNews::Run { news, .. }) if onto(news))
    };
    let folded = news.len();
    assert!(
        news.iter().any(handed_on),
        "caught up on {folded} pieces, none a successor"
    );
    assert_eq!(*in_view(&state).run.run(), successor);
}
