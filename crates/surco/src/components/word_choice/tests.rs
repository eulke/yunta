//! A row of words with one chosen is a block every surface draws: as Markdown, as a page,
//! and on a terminal in every environment.

use std::path::{Path, PathBuf};

use super::WordChoice;
use crate::doc::Doc;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::look::Look;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/word_choice")
}

/// `words` in a row, the one at `chosen` chosen.
fn row(words: &[&str], chosen: usize, focused: bool) -> WordChoice {
    WordChoice {
        words: words.iter().map(|word| word.to_string()).collect(),
        chosen,
        focused,
    }
}

/// A new run's modes while the keys change them, the second chosen: the pointer stands
/// between two words.
pub(crate) fn modes() -> WordChoice {
    row(&["quick", "standard", "full"], 1, true)
}

/// A new run's adapters while the keys act on another row, the first chosen: a word of
/// several words among them.
fn adapters() -> WordChoice {
    row(
        &["as the runners in config.yaml say", "claude-code", "codex"],
        0,
        false,
    )
}

#[test]
fn a_row_of_words_matches_its_goldens_on_every_surface() {
    for (case, choice) in [("modes", modes()), ("adapters", adapters())] {
        let doc = Doc::new().with(choice);
        assert_golden(&goldens().join(format!("{case}.md")), &Markdown.draw(&doc));
        assert_golden(&goldens().join(format!("{case}.html")), &Html.body(&doc));
        for environment in ENVIRONMENTS.iter().chain(&RICH) {
            let drawn = Terminal::on(Look::of(environment)).draw(&doc);
            assert_golden(&environment.golden(&goldens(), case), &drawn);
        }
    }
}
