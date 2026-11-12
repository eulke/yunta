//! A typed answer is a block every surface draws: as Markdown, as a page, and on a
//! terminal in every environment.

use std::path::{Path, PathBuf};

use super::TypedAnswer;
use crate::components::TypedRow;
use crate::doc::Doc;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/typed_answer")
}

/// A project's name asked, its detected value standing, and a name half typed over it.
pub(crate) fn named() -> TypedAnswer {
    TypedAnswer {
        asked: Line::new()
            .push(Tone::Strong, "project name")
            .push(Tone::Muted, " (enter keeps `yunta`)"),
        typed: TypedRow {
            prompt: "> ".to_string(),
            before: "yunta-".to_string(),
            after: "tui".to_string(),
        },
    }
}

#[test]
fn a_typed_answer_matches_its_goldens_on_every_surface() {
    let doc = Doc::new().with(named());
    assert_golden(&goldens().join("named.md"), &Markdown.draw(&doc));
    assert_golden(&goldens().join("named.html"), &Html.body(&doc));
    for environment in ENVIRONMENTS.iter().chain(&RICH) {
        let drawn = Terminal::on(Look::of(environment)).draw(&doc);
        assert_golden(&environment.golden(&goldens(), "named"), &drawn);
    }
}

/// Held, the cursor stands after what was typed before it, on the row under the
/// question.
#[test]
fn a_held_answer_puts_the_cursor_where_the_typing_is() {
    let look = Look::of(&ENVIRONMENTS[0]);
    let held = named().drawn(&look);
    assert_eq!(held.cursor, Some((1, "> yunta-".len())));
}
