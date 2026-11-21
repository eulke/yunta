//! A run parked on its child, through `State` alone, in what its frames cannot hold: while
//! the run leads, its child is the run read whole, shown in its place and followed under
//! every stage opened over it, the trail leading up to the parent through the node that bore
//! the child; `esc` puts the child's card aside, on the child's node, and with no prompt
//! standing `esc` goes up to the parent, whose node pointer is the person's on that node, and
//! `f` gives the parent the lead back, its child in its place again.

use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;

use super::{scene_of, trail};

/// The trail of the small run's child, shown in its parent's place: the parent, the node that
/// bore the child, the child, and the node the child's pointer stands on.
const THE_CHILDS: [&str; 5] = ["yunta", "PARENT", "feat", "nted-1", "approve"];

/// The handle of the run `screen` follows, where it follows one.
fn followed(screen: &RunScreen) -> Option<String> {
    let run = screen.state().followed().run;
    run.map(|run| run.handle().to_string())
}

#[tokio::test]
async fn a_child_stands_in_its_parents_place_while_the_parent_leads() {
    let mut screen = run::opened("child").await;
    assert_eq!(followed(&screen).as_deref(), Some("nted-1"));
    assert_eq!(trail(&screen), THE_CHILDS);
    assert_eq!(scene_of(&screen), Some("a card on what the gate shows"));
    // A stage opened over the child's screen: the child is what is followed under it.
    screen.pressed(&[Stroke::Insert('s')]).await;
    assert_eq!(followed(&screen).as_deref(), Some("nted-1"));
    screen.pressed(&[Stroke::Decline]).await;
    assert_eq!(trail(&screen), THE_CHILDS);
}

#[tokio::test]
async fn esc_on_a_child_goes_up_to_its_parent_on_the_node_that_bore_it() {
    let mut screen = run::opened("child").await;
    screen.pressed(&[Stroke::Decline]).await;
    assert_eq!(followed(&screen).as_deref(), Some("nted-1"));
    assert_eq!(trail(&screen), THE_CHILDS);
    assert_eq!(
        scene_of(&screen),
        Some("the node's own"),
        "its card put aside, the person points at the node it waits at"
    );
    screen.pressed(&[Stroke::Decline]).await;
    assert_eq!(followed(&screen).as_deref(), Some("PARENT"));
    assert_eq!(trail(&screen), ["yunta", "PARENT", "feat"]);
    assert_eq!(
        scene_of(&screen),
        Some("the node's own"),
        "the person points at the node that bore the child"
    );
    screen.pressed(&[Stroke::Insert('f')]).await;
    assert_eq!(followed(&screen).as_deref(), Some("nted-1"));
    assert_eq!(trail(&screen), THE_CHILDS);
}
