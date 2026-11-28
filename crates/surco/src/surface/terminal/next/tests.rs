//! The commands that do at a shell what an act does, at a terminal: each on its row, muted,
//! with no heading over them and no gloss beside them; a command wider than the row broken
//! where a shell reads it as the same command. Every expected row is written by hand.

use std::path::{Path, PathBuf};

use crate::components::SameAtAShell;
use crate::doc::{Block, Doc};
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/same_at_a_shell")
}

/// `commands`, each as a shell takes it.
fn same(commands: &[&str]) -> SameAtAShell {
    SameAtAShell(commands.iter().map(|command| command.to_string()).collect())
}

/// What closing a run does at a shell.
fn closing() -> SameAtAShell {
    same(&["yunta close EZRPE9"])
}

/// What installing a pack and setting the repository up do at a shell: the first command
/// wider than a narrow row.
fn installing() -> SameAtAShell {
    same(&[
        "yunta pack add https://github.com/eulke/yunta//packs/fragua@v0.0.5",
        "yunta init",
    ])
}

/// The rows `same` draws `cells` wide, unpainted.
fn drawn(same: SameAtAShell, cells: usize) -> Vec<Line> {
    drawn_within(same, Width::exact(cells))
}

/// The rows `same` draws at `width`, unpainted.
fn drawn_within(same: SameAtAShell, width: Width) -> Vec<Line> {
    let look = Look {
        glyphs: crate::glyphs::Glyphs::Unicode,
        paint: Paint::Plain,
        width,
        motion: crate::motion::Motion::Still,
    };
    Terminal::on(look).block(&Block::from(same))
}

/// The text of each row `same` draws at `width`.
fn texts_within(same: SameAtAShell, width: Width) -> Vec<String> {
    drawn_within(same, width).iter().map(Line::text).collect()
}

/// Each command stands on its row from the row's first cell, muted, with no `next` heading
/// over it and no gloss beside it.
#[test]
fn each_command_stands_on_its_row_with_no_heading_and_no_gloss() {
    let rows = drawn(installing(), 80);
    let texts: Vec<String> = rows.iter().map(Line::text).collect();
    assert_eq!(
        texts,
        [
            "yunta pack add https://github.com/eulke/yunta//packs/fragua@v0.0.5",
            "yunta init",
        ]
    );
    for row in &rows {
        assert!(
            row.spans()
                .iter()
                .filter(|span| !span.text.trim().is_empty())
                .all(|span| span.tone == Tone::Muted),
            "{row:?}"
        );
    }
}

/// A command wider than its row goes on in the rows under it, each but the last ending in
/// ` \` and each after the first set in by two, so a shell still reads one command.
#[test]
fn a_command_wider_than_its_row_is_broken_as_a_shell_reads_it() {
    let texts: Vec<String> = drawn(installing(), 60).iter().map(Line::text).collect();
    assert_eq!(
        texts,
        [
            "yunta pack add \\",
            "  https://github.com/eulke/yunta//packs/fragua@v0.0.5",
            "yunta init",
        ]
    );
}

/// A word wider than the width words wrap at stands whole in the cell lent past it, as a box
/// lends the cell before its far upright: the setup's link in a box of 56 cells.
#[test]
fn a_word_wider_than_the_words_stands_whole_in_the_cell_lent() {
    assert_eq!(
        texts_within(installing(), Width::exact(52).lending(1)),
        [
            "yunta pack add \\",
            "  https://github.com/eulke/yunta//packs/fragua@v0.0.5",
            "yunta init",
        ]
    );
}

/// A word wider even than the cell lent lets it is cut at the row's end, never parted onto a
/// row of its own: a link one cell longer than the setup's loses its version whole, the cut
/// falling before the number.
#[test]
fn a_word_wider_than_the_cell_lent_is_cut_before_its_number() {
    let longer = same(&["yunta pack add https://github.com/eulke/yunta//packs/fragua@v0.0.15"]);
    assert_eq!(
        texts_within(longer, Width::exact(52).lending(1)),
        [
            "yunta pack add \\",
            "  https://github.com/eulke/yunta//packs/fragua@v…",
        ]
    );
}

/// A command of several words folds between two of them, the row it leaves ending ` \`, and
/// a quoted stretch stays one word.
#[test]
fn a_command_of_several_words_folds_between_them() {
    let adjusting = same(&[
        "yunta resolve-gate 7E5PH4 adjust --text \"keep the plan but drop the second task\"",
    ]);
    assert_eq!(
        texts_within(adjusting, Width::exact(52).lending(1)),
        [
            "yunta resolve-gate 7E5PH4 adjust --text \\",
            "  \"keep the plan but drop the second task\"",
        ]
    );
}

/// A printed page is a stream: a row may pass its width, which the terminal wraps, so a word
/// of a command wider than the page stands whole on its row, to be copied whole into another
/// terminal; the words before it fold between them as anywhere.
#[test]
fn on_a_page_a_word_wider_than_the_page_stands_whole_past_it() {
    let link = "https://github.com/eulke/a-repository-with-a-long-name//packs/fragua@v0.0.15";
    let doc = Doc::new().with(same(&[&format!("yunta pack add {link}")]));
    let page = Terminal::on(Look::of(&ENVIRONMENTS[2])).draw(&doc);
    assert_eq!(page, format!("yunta pack add \\\n  {link}\n"));
}

/// No command, no row: a confirmation that names none says nothing of a shell.
#[test]
fn no_command_draws_no_row() {
    assert!(drawn(same(&[]), 60).is_empty());
}

#[test]
fn the_commands_match_their_goldens_on_every_surface() {
    for (case, same) in [("closing", closing()), ("installing", installing())] {
        let doc = Doc::new().with(same);
        assert_golden(&goldens().join(format!("{case}.md")), &Markdown.draw(&doc));
        assert_golden(&goldens().join(format!("{case}.html")), &Html.body(&doc));
        for environment in ENVIRONMENTS.iter().chain(&RICH) {
            let drawn = Terminal::on(Look::of(environment)).draw(&doc);
            assert_golden(&environment.golden(&goldens(), case), &drawn);
        }
    }
}
