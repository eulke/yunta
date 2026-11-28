//! What a surface shows where it has less room than it lays a screen out in, and what a key
//! does while it is short of it.

use surco::surface::{Markdown, Surface};

mod support;

/// The document says the room needed and the room there is.
#[test]
fn the_room_needed_says_both_sizes() {
    let said = Markdown.draw(&yunta_ui::room_needed((60, 14), (59, 14)));
    assert!(said.contains("60 by 14"), "{said}");
    assert!(said.contains("59 by 14"), "{said}");
}

/// While the surface is short of the room it lays a screen out in, its screen is the room it
/// needs alone, which says only leaving: `q` and Ctrl-C leave, and no other key reaches the
/// stage under it, here a first request a key types into. With the room back, keys reach it
/// again.
#[tokio::test]
async fn short_of_room_only_leaving_answers_a_key() {
    use surco::look::Look;
    use yunta_ui::prompt::Stroke;
    use yunta_ui::{Input, Request, Room};

    let (mut state, _world) = support::on_the_field(&[]).await;
    let typed = |state: &yunta_ui::State| Markdown.screen(&state.screen(&Look::plain()));
    state.update(Input::Room(Room::Short));
    for stroke in "zzz".chars().map(Stroke::Insert) {
        assert_eq!(
            state.update(Input::Stroke(stroke)),
            [],
            "no key reaches the stage"
        );
    }
    assert_eq!(
        state.update(Input::Stroke(Stroke::Insert('q'))),
        [Request::Leave]
    );
    assert_eq!(
        state.update(Input::Stroke(Stroke::Interrupt)),
        [Request::Leave]
    );
    state.update(Input::Room(Room::Enough));
    assert!(!typed(&state).contains("zzz"), "{}", typed(&state));
    for stroke in "zzq".chars().map(Stroke::Insert) {
        state.update(Input::Stroke(stroke));
    }
    assert!(typed(&state).contains("zzq"), "{}", typed(&state));
}
