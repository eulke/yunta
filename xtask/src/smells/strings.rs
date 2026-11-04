//! The counters over the text a production string carries: its spacing,
//! its plurals, the values it formats with `Debug`, and a dash a reader
//! would be told, the CLI's help among it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::corpus::{is_test_source, Corpus};
use super::read_corpus;
use super::shape::{
    has_dash, has_debug_format, has_doc_dash, has_inner_space_run, has_parenthesized_plural,
    production_only,
};

/// The counters over the text a production string carries: its spacing, its
/// plurals and the values it formats with `Debug`.
pub(super) fn string_counts(corpus: &Corpus, counts: &mut BTreeMap<String, usize>) {
    // A run of spaces inside a message is a `\` continuation that went
    // missing: the reader gets the source's indentation in the text.
    counts.insert(
        "space_runs_in_prod_strings".to_string(),
        corpus
            .prod
            .iter()
            .flat_map(|text| text.lines())
            .filter(|line| has_inner_space_run(line))
            .count(),
    );
    // A count hedged with `(s)` while its number stands beside it: the
    // message says `3 file(s)` where it knows to say `3 files`.
    counts.insert(
        "parenthesized_plural_in_prod_strings".to_string(),
        corpus
            .prod
            .iter()
            .flat_map(|text| text.lines())
            .filter(|line| has_parenthesized_plural(line))
            .count(),
    );
    // A value formatted with `Debug` in text a reader is told. Counted over
    // the crates that ship: a test-support crate talks to the one who wrote
    // the code.
    counts.insert(
        "debug_format_reaching_reader".to_string(),
        production_lines(
            &corpus.src,
            |path| path.to_string_lossy().contains("crates/testkit"),
            has_debug_format,
        ),
    );
    // A dash in what a reader is told: a sentence says its parts with the
    // punctuation every terminal draws and every reader reads, and the one
    // dash a surface draws is the glyph set's. The CLI's doc comments are
    // told too: clap prints them as each command's help.
    counts.insert(
        "dash_in_prod_strings".to_string(),
        production_lines(&corpus.src, in_glyphs, has_dash)
            + production_lines(&corpus.src, |path| !declares_commands(path), has_doc_dash),
    );
}

/// Whether `path` is where the CLI declares its commands, whose doc comments
/// clap prints as their help.
fn declares_commands(path: &Path) -> bool {
    path.to_string_lossy().contains("crates/cli/src/cli/")
}

/// The production lines of `files` that `holds` is true of, a test source
/// and a file `skip` names aside.
pub(super) fn production_lines(
    files: &[PathBuf],
    skip: impl Fn(&Path) -> bool,
    holds: impl Fn(&str) -> bool,
) -> usize {
    files
        .iter()
        .filter(|path| !skip(path) && !is_test_source(path))
        .map(|path| {
            production_only(&read_corpus(path))
                .lines()
                .filter(|line| holds(line))
                .count()
        })
        .sum()
}

/// Whether `path` is the one file a glyph is spelled in.
pub(super) fn in_glyphs(path: &Path) -> bool {
    path.ends_with("surco/src/glyphs.rs")
}
