//! The target screens' states: each shows what it is for, and two builds of it are the
//! same, so a frame drawn of it reads alike on every machine.

use std::future::Future;

use surco::doc::Block;
use surco::surface::Markdown;
use yunta_testkit_ui::screens;
use yunta_ui::State;

fn markdown(state: &State) -> String {
    Markdown.screen(&state.screen(&Markdown.look()))
}

/// The state `built` makes, built twice: its screen the same both times.
async fn built_alike<F: Future<Output = State>>(built: impl Fn() -> F) -> State {
    let first = built().await;
    let again = built().await;
    assert_eq!(markdown(&first), markdown(&again), "two builds differ");
    first
}

#[test]
fn each_target_screen_shows_what_it_is_for_and_is_built_alike() {
    yunta_testkit::on_a_deep_stack(|| async {
        let field = markdown(&built_alike(screens::field).await);
        assert!(field.contains("runs of this repository"), "{field}");
        assert!(field.contains("**Q78GRR** needs you"), "{field}");

        let run = built_alike(screens::run).await;
        let screen = run.screen(&Markdown.look());
        assert!(screen.about.is_some(), "the run's ribbon stands across it");
        assert!(screen.side.is_some(), "the page of a node stands beside it");
        assert!(
            markdown(&run).contains("1 of 2 nodes done"),
            "{}",
            markdown(&run)
        );

        let card = built_alike(screens::card).await;
        let blocks = card.screen(&Markdown.look()).main.doc.into_blocks();
        assert!(
            matches!(blocks.first(), Some(Block::Card(_))),
            "the card stands first"
        );

        let desk = built_alike(screens::desk).await;
        let blocks = desk.screen(&Markdown.look()).main.doc.into_blocks();
        assert!(
            matches!(blocks.first(), Some(Block::Desk(_))),
            "the desk takes the screen"
        );

        let confirmation = markdown(&built_alike(screens::confirmation).await);
        assert!(confirmation.contains("cancel Q78GRR?"), "{confirmation}");

        let palette = markdown(&built_alike(screens::palette).await);
        assert!(palette.contains("## what you can do here"), "{palette}");

        let help = markdown(&built_alike(screens::help).await);
        assert!(help.contains("## the keys here"), "{help}");
    });
}
