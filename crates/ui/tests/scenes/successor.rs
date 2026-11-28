//! A run that closed promoted, through `State` alone, in what its frames cannot hold: its
//! successor takes the screen while the run leads; where the person holds the node pointer
//! as the promotion comes, the screen stays on the run, which is what the application
//! follows, until `f` gives the lead back and the successor takes the screen then; and a
//! page written for the run keeps naming it as the successor takes its place under the page.

use std::sync::Arc;

use surco::look::Look;
use yunta_core::events::Channel;
use yunta_core::step::NextStep;
use yunta_testkit_ui::cut::{Cut, Stands, TARGET_CUTS};
use yunta_testkit_ui::driving::driven;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{ItsRun, Performer, State};

use super::{pane_drawn, scene_of, trail};

/// The handle of the run `screen`'s application follows.
fn followed(screen: &RunScreen) -> Option<String> {
    let run = screen.state().followed().run;
    run.map(|run| run.handle().to_string())
}

#[tokio::test]
async fn with_the_node_pointer_the_persons_a_promotion_leaves_the_screen_on_the_run() {
    let mut screen = run::opened("promoted").await;
    assert_eq!(followed(&screen).as_deref(), Some("PR0M0T"));
    assert_eq!(trail(&screen), ["yunta", "PR0M0T", "lint"]);
    assert_eq!(scene_of(&screen), Some("ended"));
    // Its pane, under the title of the node its end names: why it promoted, then the
    // successor kept beside it.
    let pane = pane_drawn(&screen, 84, 26);
    for row in [
        "node `lint` exhausted its reroutes to `fix-lint` (exit 1)",
        "its successor",
    ] {
        assert!(
            pane.iter().any(|drawn| drawn == row),
            "no `{row}` in {pane:#?}"
        );
    }
    screen.pressed(&[Stroke::Insert('f')]).await;
    assert_eq!(
        followed(&screen).as_deref(),
        Some("nted-1"),
        "`f` gives the lead back, and the successor takes the screen"
    );
    assert_eq!(trail(&screen)[..2], ["yunta", "nted-1"]);
}

/// The page for a browser written for a run is a record of that run: where the run closes
/// promoted while the page stands over it and its successor takes the run's place under
/// the page, the page keeps naming the run it was written for while the application
/// follows the successor, and `esc` goes back to the successor's screen.
#[tokio::test]
async fn a_page_written_for_the_run_keeps_naming_it_as_its_successor_takes_its_place() {
    let target = TARGET_CUTS
        .iter()
        .find(|target| target.name == "promoted")
        .expect("the promoted run's cut");
    let Stands::Run(shown) = target.stands else {
        panic!("the promoted run's screen");
    };
    let cut = Cut::of(&[shown]).await;
    let promoted = cut.runs()[0].clone();
    let performer = Arc::new(Performer::new(cut.workspace(), Channel::Tty));
    let (opened, asked) = State::opened_on(cut.now(), &NextStep::status(&promoted))
        .expect("the run's stage takes its status");
    let page = [Stroke::Insert('o')];
    let state = driven(opened, asked, &performer, &page).await.state;
    assert_eq!(state.its_run(), Some(ItsRun::WroteFor(promoted.clone())));
    let promotion = target.goes_on_to.expect("the cut goes on to the promotion");
    cut.goes_on(shown, shown.seq, promotion).await;
    let state = driven(state, Vec::new(), &performer, &[]).await.state;
    let successor = state.followed().run.expect("a run followed");
    assert_ne!(successor, promoted, "the successor takes the run's place");
    assert_eq!(
        state.its_run(),
        Some(ItsRun::WroteFor(promoted.clone())),
        "the page keeps naming the run it was written for"
    );
    let trail = state.screen(&Look::plain()).trail;
    assert_eq!(
        trail[1],
        format!("{} {}", promoted.handle(), yunta_render::screen::PAGE),
        "the page's place is the run it was written for and its word"
    );
    let back = driven(state, Vec::new(), &performer, &[Stroke::Decline]).await;
    assert_eq!(back.state.its_run(), Some(ItsRun::Shows(successor)));
}
