//! The counters of what the design language says of itself: a glyph is written in one file,
//! a state word in one module, and a block holds no width, escape or glyph of its own; and
//! of the one rule the command line adds: a command a person types is spelled in one module.

use std::path::Path;

use super::corpus::{crate_src_dirs, is_test_source, rs_files};
use super::read_corpus;
use super::shape::production_only;
use super::sites::Counter;

/// Every counter this module measures.
pub const COUNTERS: &[Counter] = &[
    ("glyph_literal_outside_glyphs", count_glyphs),
    ("command_outside_advice", count_commands),
    ("state_phrase_outside_words", count_state_phrases),
    ("look_inside_a_block", count_look_inside_a_block),
];

/// The production lines under `files` that `holds` is true of, comments aside, in files `skip`
/// does not name.
fn lines_of(
    files: impl IntoIterator<Item = std::path::PathBuf>,
    skip: impl Fn(&str) -> bool,
    holds: impl Fn(&str) -> bool,
) -> usize {
    files
        .into_iter()
        .filter(|path| {
            !is_test_source(path) && !shown(path).contains("/tests/") && !skip(&shown(path))
        })
        .map(|path| {
            production_only(&read_corpus(&path))
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .map(|line| line.split(" // ").next().unwrap_or(line))
                .filter(|code| holds(code))
                .count()
        })
        .sum()
}

/// A path with forward slashes, as a fragment names it.
fn shown(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Every source file of every crate that ships. The testkit talks to the one who wrote a test.
fn shipped(root: &Path) -> Vec<std::path::PathBuf> {
    crate_src_dirs(root)
        .iter()
        .flat_map(|dir| rs_files(dir))
        .filter(|path| !shown(path).contains("crates/testkit"))
        .collect()
}

/// A glyph written outside the one file that spells them: a surface drawing in the ASCII set
/// prints it anyway, and two spellings of one mark drift apart. A glyph is any visible
/// character outside ASCII that is no letter or digit.
fn count_glyphs(root: &Path) -> usize {
    lines_of(
        shipped(root),
        |path| path.ends_with("surco/src/glyphs.rs"),
        holds_a_glyph,
    )
}

/// Whether `code` writes a glyph.
pub(super) fn holds_a_glyph(code: &str) -> bool {
    code.chars()
        .any(|c| !c.is_ascii() && !c.is_alphanumeric() && !c.is_whitespace() && !c.is_control())
}

/// A command written in the command line outside `commands::advice`, the one place that knows
/// how a person types each: one spelled twice is one that drifts. Clap's own declarations of
/// the commands are what they are named by.
fn count_commands(root: &Path) -> usize {
    lines_of(
        rs_files(&root.join("crates/cli/src")),
        |path| path.contains("commands/advice") || path.contains("crates/cli/src/cli/"),
        spells_a_command,
    )
}

/// Whether `code` spells a command: `yunta` and a word, opening a string or in a code span.
pub(super) fn spells_a_command(code: &str) -> bool {
    ["`yunta ", "\"yunta "].iter().any(|mark| {
        code.match_indices(mark)
            .any(|(at, _)| code[at + mark.len()..].starts_with(|c: char| c.is_ascii_lowercase()))
    })
}

/// A word for where a run or a node stands, written outside `surco::words`: told in two places,
/// it reads two ways the day one of them changes.
fn count_state_phrases(root: &Path) -> usize {
    lines_of(
        shipped(root),
        |path| path.contains("surco/src/words"),
        says_a_state,
    )
}

/// Whether `code` says one of the words only a state is called by, in a sentence: `rerouted`
/// alone is also a name in code, so it counts with what follows it in a sentence.
pub(super) fn says_a_state(code: &str) -> bool {
    ["needs you", "never ran", "rerouted to"]
        .iter()
        .any(|phrase| code.contains(phrase))
}

/// A block holding what is the surface's to decide: a width, an escape or one glyph set. A
/// surface hands a component its glyphs, and the catalog is the samples, which are drawn.
fn count_look_inside_a_block(root: &Path) -> usize {
    lines_of(
        rs_files(&root.join("crates/surco/src/components")),
        |path| path.contains("components/catalog"),
        holds_a_look,
    )
}

/// Whether `code` decides how it is drawn.
pub(super) fn holds_a_look(code: &str) -> bool {
    [
        "Width",
        "cell_width",
        ".width",
        "Glyphs::Unicode",
        "Glyphs::Ascii",
        "\\x1b",
        "\\u{1b}",
    ]
    .iter()
    .any(|mark| code.contains(mark))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glyph_is_a_visible_character_that_is_no_letter() {
        assert!(holds_a_glyph("format!(\"{kind} → {tool}\")"));
        assert!(holds_a_glyph("'…'"));
        assert!(!holds_a_glyph("\"plain words\""));
        assert!(!holds_a_glyph("\"señor\""));
    }

    #[test]
    fn a_command_is_yunta_and_a_word_opening_a_string_or_in_a_code_span() {
        assert!(spells_a_command("\"run `yunta doctor` for detail\""));
        assert!(spells_a_command("format!(\"yunta close {run}\")"));
        assert!(!spells_a_command("\"what yunta does\""));
        assert!(!spells_a_command("\"`yunta`\""));
    }

    #[test]
    fn a_state_is_said_in_a_sentence_and_a_name_in_code_is_not_one() {
        assert!(says_a_state("\"it needs you\""));
        assert!(says_a_state("format!(\"rerouted to `{to}`\")"));
        assert!(!says_a_state("let rerouted = node.reroutes > 0;"));
    }

    #[test]
    fn a_block_is_handed_its_glyphs_and_decides_nothing_of_how_it_is_drawn() {
        assert!(!holds_a_look(
            "pub fn table(&self, glyphs: Glyphs) -> Table {"
        ));
        assert!(holds_a_look("let row = Glyphs::Unicode.branch(false);"));
        assert!(holds_a_look("let room = look.width.cells();"));
    }
}
