//! A choice list is a block every surface draws: as Markdown, as a page, and on a
//! terminal in every environment; and held, it gives way to the rows it has.

use std::path::{Path, PathBuf};

use super::ChoiceList;
use crate::components::{ChoiceRow, Choices, Legend};
use crate::doc::Doc;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::look::Look;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/choice_list")
}

/// The shapes a new workflow starts as, narrowed by what was typed to the two that
/// hold it, the second pointed at.
pub(crate) fn shapes() -> ChoiceList {
    let shape = |name: &str, said: &str| ChoiceRow {
        name: name.to_string(),
        said: Some(said.to_string()),
        cost: None,
    };
    ChoiceList {
        choices: Some(Choices {
            rows: vec![
                shape("one-node", "one command whose exit code is the check"),
                shape("lint-fix", "a check, and an agent that fixes what it finds"),
            ],
            at: 1,
        }),
        filter: Some("n".to_string()),
        legend: Legend::new()
            .arrows("move")
            .typing("to filter")
            .key("enter", "chooses")
            .key("esc", "takes the first"),
    }
}

#[test]
fn a_choice_list_matches_its_goldens_on_every_surface() {
    let doc = Doc::new().with(shapes());
    assert_golden(&goldens().join("shapes.md"), &Markdown.draw(&doc));
    assert_golden(&goldens().join("shapes.html"), &Html.body(&doc));
    for environment in ENVIRONMENTS.iter().chain(&RICH) {
        let drawn = Terminal::on(Look::of(environment)).draw(&doc);
        assert_golden(&environment.golden(&goldens(), "shapes"), &drawn);
    }
}

/// A list a filter leaves empty says so, on every surface.
#[test]
fn a_list_the_filter_empties_says_nothing_matches() {
    let empty = ChoiceList {
        choices: None,
        ..shapes()
    };
    let doc = Doc::new().with(empty);
    assert!(Markdown.draw(&doc).contains("nothing matches"));
    let look = Look::of(&ENVIRONMENTS[0]);
    let drawn = Terminal::on(look).draw(&doc);
    assert!(drawn.contains("nothing matches"), "{drawn}");
}
