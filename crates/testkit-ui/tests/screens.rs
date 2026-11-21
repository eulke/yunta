//! The target screens' states: each shows what it is for, and two builds of it are the
//! same, so a frame drawn of it reads alike on every machine.

use std::future::Future;

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
    });
}
