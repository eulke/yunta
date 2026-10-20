//! `cargo xtask smells [--check]` — the smell ratchet.
//!
//! The audit that this repository's rebuild answered measured a set of
//! patterns (unwrap outside tests, `Result<_, String>`, wall-clock reads,
//! git runners, discarded results, oversized files and functions, copied
//! test helpers). Each is now at or near zero, held there by typed errors,
//! injected clocks, one git module, and a shared test crate. This ratchet
//! keeps them there: it counts every pattern against a committed baseline
//! and, under `--check`, fails naming any count that rose. A number can
//! only ever go down — the day one does, the baseline moves with it in the
//! same change.
//!
//! Beside them it counts the rules this repository states about itself:
//! the words it retired, the tense its texts are written in, the sites
//! that own a capability ([`sites`]), the shape of a test, the documents
//! its code names, and a threshold with no reason written beside it. A
//! rule nothing measures is a rule a change is free to break.
//!
//! Counting is line-based (a line matching the pattern), the same measure
//! the rebuild's own acceptance commands used, so a contributor and CI see
//! the same number `grep -c` would.

mod baseline;
mod cli;
mod corpus;
mod disk;
mod prose;
mod references;
mod scan;
mod shape;
mod sites;
mod thresholds;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use yunta_core::durable::{self, Durability};

use corpus::{crate_src_dirs, crate_test_dirs, is_test_source, rs_files, Corpus};
use disk::{count_engine_fs_outside_runtime, count_sync_fs_in_async, discarded_results};
use scan::blanked_lines;
use shape::{
    functions_over, has_debug_format, has_inner_space_run, has_parenthesized_plural,
    module_doc_lines, production_only,
};
use thresholds::numeric_consts_without_reason;

/// Where the committed baseline lives, next to this crate.
fn baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("smells.baseline")
}

/// The workspace root — the parent of this crate's directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// A file the ratchet cannot read stops the measure, never reads clean.
pub(super) fn read_corpus(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("the ratchet cannot read `{}`: {error}", path.display()))
}

/// Lines across `files` that satisfy `matches`.
fn count_lines(files: &[PathBuf], matches: impl Fn(&str) -> bool) -> usize {
    files
        .iter()
        .map(|path| read_corpus(path))
        .map(|text| text.lines().filter(|line| matches(line)).count())
        .sum()
}

/// Every counter, by name, computed fresh from the tree.
fn measure() -> BTreeMap<String, usize> {
    let root = workspace_root();
    let corpus = Corpus::of(&root);
    let mut counts = BTreeMap::new();
    code_counts(&corpus, &mut counts);
    string_counts(&corpus, &mut counts);
    text_counts(&root, &corpus, &mut counts);
    test_counts(&corpus, &mut counts);
    // The counters that read one question each, from the tree they name.
    for (name, count) in COUNTERS.iter().chain(sites::COUNTERS) {
        counts.insert((*name).to_string(), count(&root));
    }
    counts
}

/// The counters over how production code is shaped: what it discards, and how
/// long its functions and files run.
fn code_counts(corpus: &Corpus, counts: &mut BTreeMap<String, usize>) {
    // Production `unwrap`/`expect`/`panic`/indexing are not counted here:
    // the crate-root `#![deny(clippy::…)]` lints make each a compile error,
    // which CI's `cargo clippy -- -D warnings` already enforces — a stronger
    // gate than a ratchet, and one a comment mention of `.unwrap()` can't
    // inflate.

    // A discarded `Result` hides a degradation the log should carry.
    counts.insert(
        "let_underscore_in_prod".to_string(),
        corpus.prod.iter().map(|text| discarded_results(text)).sum(),
    );
    // A production function near 50 lines is a seam not yet cut.
    counts.insert(
        "prod_fns_over_50_lines".to_string(),
        corpus
            .prod
            .iter()
            .map(|text| functions_over(text, 50))
            .sum(),
    );
    // A production function of 100 lines or more is the hard limit: there
    // is no exception to it.
    counts.insert(
        "prod_fns_over_100_lines".to_string(),
        corpus
            .prod
            .iter()
            .map(|text| functions_over(text, 100))
            .sum(),
    );
    // A file near 500 lines is another (counted whole — its tests are part
    // of what a reader scrolls).
    counts.insert(
        "src_files_over_500_lines".to_string(),
        corpus
            .src
            .iter()
            .filter(|path| read_corpus(path).lines().count() > 500)
            .count(),
    );
}

/// The counters over the text a production string carries: its spacing, its
/// plurals and the values it formats with `Debug`.
fn string_counts(corpus: &Corpus, counts: &mut BTreeMap<String, usize>) {
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
        corpus
            .src
            .iter()
            .filter(|path| {
                let shown = path.to_string_lossy();
                !shown.contains("crates/testkit") && !is_test_source(path)
            })
            .map(|path| read_corpus(path))
            .map(|text| {
                production_only(&text)
                    .lines()
                    .filter(|line| has_debug_format(line))
                    .count()
            })
            .sum(),
    );
}

/// The counters over the text a reader is shown: the glyphs a line is drawn
/// with, a ledger's fold, and the words and tense the repository states.
fn text_counts(root: &Path, corpus: &Corpus, counts: &mut BTreeMap<String, usize>) {
    // A glyph that says how a line reads — the separator between two facts,
    // the mark of a line that goes on — written as a literal: a surface
    // drawing with the ASCII set prints it nonetheless, where the glyph set would
    // have printed what that terminal can draw.
    counts.insert(
        "separator_literal_outside_glyphs".to_string(),
        corpus
            .drawn
            .iter()
            .filter(|path| !path.ends_with("render/src/glyphs.rs") && !is_test_source(path))
            .map(|path| read_corpus(path))
            .map(|text| {
                production_only(&text)
                    .lines()
                    .filter(|line| {
                        !line.trim_start().starts_with("//") && line.contains(['·', '↪'])
                    })
                    .count()
            })
            .sum(),
    );
    // A `_ =>` inside a ledger's fold is a kind that derives nothing with
    // nothing saying so: the arm exists, the reader sees no diagnostic, and
    // the state is simply wrong. Every kind is named, and one that moves
    // nothing is listed by name in the replay test.
    counts.insert(
        "wildcard_in_ledger_apply".to_string(),
        wildcards_in_folds(root),
    );
    // A word this repository retired still naming the thing it retired it
    // for, and a text that describes a plan or a past instead of what the
    // repository does. Both read prose, over a corpus wider than `.rs`.
    counts.insert(
        "banned_vocabulary".to_string(),
        prose::banned_vocabulary(root),
    );
    counts.insert("tense_markers".to_string(), prose::tense_markers(root));
    // The repository writes English; the design records it still keeps are
    // the one place Spanish stands.
    counts.insert(
        "spanish_prose_lines".to_string(),
        prose::spanish_prose_lines(root),
    );
}

/// The counters over test code kept outside the support crate.
fn test_counts(corpus: &Corpus, counts: &mut BTreeMap<String, usize>) {
    // Test infrastructure lives in the support crate, never copied into a
    // test that needs it. Counted over `src` as well as `tests`, because a
    // `#[cfg(test)]` module inside a crate is a test that needs it too, and
    // a copy there drifts from the one place just as quietly.
    let test_code: Vec<PathBuf> = corpus.tests.iter().chain(&corpus.src).cloned().collect();
    counts.insert(
        "copied_test_helpers".to_string(),
        count_lines(&test_code, |line| {
            let t = line.trim_start();
            t.starts_with("fn git(")
                || t.starts_with("fn yunta_in(")
                || t.starts_with("fn init_repo(")
                || t.starts_with("struct FixedClock")
        }),
    );
}

/// The counters that read the corpus of code and nothing else, beside the
/// ones [`sites`] declares.
const COUNTERS: &[sites::Counter] = &[
    ("doc_reference", references::count_doc_references),
    ("sync_fs_in_async", count_sync_fs_in_async),
    ("test_files_over_500_lines", count_test_files_over_500_lines),
    ("test_fns_over_50_lines", count_test_fns_over_50_lines),
    (
        "numeric_const_without_reason",
        count_numeric_const_without_reason,
    ),
    ("engine_fs_outside_runtime", count_engine_fs_outside_runtime),
    ("module_docs_over_60_lines", count_module_docs_over_60_lines),
    ("mode_name_in_prod", count_mode_name_in_prod),
];

/// A module doc past 60 lines. A `//!` says how its module works and what a
/// change to it touches; a rule other code can break lives in the test that
/// holds it, and a module whose working needs more is two modules.
fn count_module_docs_over_60_lines(root: &Path) -> usize {
    crate_src_dirs(root)
        .iter()
        .chain([root.join("xtask/src")].iter())
        .flat_map(|dir| rs_files(dir))
        .filter(|path| !is_test_source(path))
        .filter(|path| module_doc_lines(&read_corpus(path)) > 60)
        .count()
}

/// A mode named by its word in the engine's production code. A workflow
/// declares its modes and the engine reads what the run's mode declares:
/// `quick` and `standard` are a pack's words for its own.
fn count_mode_name_in_prod(root: &Path) -> usize {
    rs_files(&root.join("crates/engine/src"))
        .iter()
        .filter(|path| !is_test_source(path))
        .map(|path| production_only(&read_corpus(path)))
        .map(|text| {
            text.lines()
                .filter(|line| {
                    ["\"quick\"", "\"standard\""]
                        .iter()
                        .any(|word| line.contains(word))
                })
                .count()
        })
        .sum()
}

/// A test file past the budget a source file has. What a reader scrolls
/// to understand a failure is the test as well as the code it drives.
fn count_test_files_over_500_lines(root: &Path) -> usize {
    test_files(root)
        .iter()
        .filter(|path| read_corpus(path).lines().count() > 500)
        .count()
}

/// A test function past the budget a production function has. A test that
/// long names more than one behaviour, and a failure in it says which
/// line broke rather than which promise.
fn count_test_fns_over_50_lines(root: &Path) -> usize {
    test_files(root)
        .iter()
        .map(|path| functions_over(&read_corpus(path), 50))
        .sum()
}

/// A threshold with no reason written beside it, over the production `src`
/// of each crate but the test harnesses.
/// The rule: a number a test writes is read beside its assertion, while a
/// number the code carries says in prose why it has its value.
fn count_numeric_const_without_reason(root: &Path) -> usize {
    crate_src_dirs(root)
        .iter()
        .filter(|dir| !dir.to_string_lossy().contains("/testkit"))
        .flat_map(|dir| rs_files(dir))
        .filter(|path| !is_test_source(path))
        .map(|path| numeric_consts_without_reason(&production_only(&read_corpus(&path))))
        .sum()
}

/// The test files: every file a crate keeps under `tests/`, and the test
/// modules kept in a file of their own under `src/`.
fn test_files(root: &Path) -> Vec<PathBuf> {
    let src: Vec<PathBuf> = crate_src_dirs(root)
        .iter()
        .flat_map(|dir| rs_files(dir))
        .collect();
    let mut files: Vec<PathBuf> = crate_test_dirs(root)
        .iter()
        .flat_map(|dir| rs_files(dir))
        .collect();
    files.extend(src.into_iter().filter(|path| is_test_source(path)));
    files
}

/// Wildcard match arms inside the functions that fold the log: every
/// `ledger.rs` under `crates/core/src/events/`, plus the engine's own
/// derivation. A `_ =>` there is a kind that derives nothing with
/// nothing saying so.
fn wildcards_in_folds(root: &Path) -> usize {
    let mut folds: Vec<PathBuf> = corpus::entries(&root.join("crates/core/src/events"))
        .into_iter()
        .map(|domain| domain.join("ledger.rs"))
        .filter(|ledger| ledger.is_file())
        .collect();
    folds.push(root.join("crates/engine/src/replay.rs"));
    folds.sort();
    folds
        .iter()
        .map(|path| wildcards_in_apply(&read_corpus(path)))
        .sum()
}

/// Wildcard arms inside a fold's `apply`, and nowhere else in the file:
/// a `match` over an `Option` or a pair of them is ordinary reading, and
/// what this measures is a kind of event nothing names.
fn wildcards_in_apply(source: &str) -> usize {
    let mut count = 0;
    let mut depth = 0usize;
    let mut inside = false;
    for code in blanked_lines(source) {
        if !inside && code.contains("fn apply") {
            inside = true;
            depth = 0;
        }
        if inside {
            if code.trim_start().starts_with("_ =>") {
                count += 1;
            }
            let before = depth;
            depth = depth + code.matches(['{', '(']).count() - code.matches(['}', ')']).count();
            if before > 0 && depth == 0 {
                inside = false;
            }
        }
    }
    count
}

/// `cargo xtask smells`: measure and lower the baseline to what the tree
/// counts. A count only falls: a rise is refused here as `--check` refuses
/// it, so a bare run never hides one in the baseline.
pub fn write() -> Result<(), String> {
    let counts = measure();
    let path = baseline_path();
    let merge_base = baseline::at_merge_base(&workspace_root())?;
    let allowed = baseline::ceiling(&read_baseline(&path)?, merge_base.as_ref());
    let risen = baseline::rises(&allowed, &counts);
    if !risen.is_empty() {
        return Err(baseline::refusal(&path, &risen));
    }
    durable::replace(
        &path,
        baseline::render(&counts).as_bytes(),
        Durability::Atomic,
    )
    .map_err(|e| format!("cannot write `{}`: {e}", path.display()))?;
    print!("{}", baseline::render(&counts));
    println!("wrote {}", path.display());
    Ok(())
}

/// `cargo xtask smells --check`: fail if any count rose above the committed
/// baseline or above the merge base's, or if the tree measures a counter the
/// baseline holds no number for.
pub fn check() -> Result<(), String> {
    let counts = measure();
    let path = baseline_path();
    let committed = read_baseline(&path)?;
    let merge_base = baseline::at_merge_base(&workspace_root())?;
    if merge_base.is_none() {
        println!("smells: no merge base with the branch it merges into; read the committed baseline alone");
    }
    let allowed = baseline::ceiling(&committed, merge_base.as_ref());
    let risen = baseline::rises(&allowed, &counts);
    if !risen.is_empty() {
        return Err(baseline::refusal(&path, &risen));
    }
    let unlisted: Vec<String> = counts
        .keys()
        .filter(|name| !committed.contains_key(*name))
        .map(|name| format!("`{name}`"))
        .collect();
    if !unlisted.is_empty() {
        return Err(format!(
            "`{}` holds no line for {} — `cargo xtask smells` writes one for a counter at zero",
            path.display(),
            unlisted.join(", ")
        ));
    }
    println!("smells: no count rose above the baseline");
    Ok(())
}

/// The committed baseline at `path`.
fn read_baseline(path: &Path) -> Result<baseline::Counts, String> {
    std::fs::read_to_string(path)
        .map(|text| baseline::parse(&text))
        .map_err(|e| format!("cannot read `{}`: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counters that stand at zero, each held there by the ratchet: a
    /// count that reached zero is listed here, so that no change can raise it.
    const ZERO_COUNTERS: &[&str] = &[
        "banned_vocabulary",
        "tense_markers",
        "sync_fs_in_async",
        "system_clock_outside_boundary",
        "let_underscore_in_prod",
        "env_read_outside_boundary",
        "exit_failure_in_cli",
        "create_run_outside_testkit",
        "execute_run_outside_testkit",
        "git_command_outside_git_rs",
        "reason_built_by_format",
        "run_tool_named_outside_vocabulary",
        "separator_literal_outside_glyphs",
        "sleep_outside_mock_latency",
        "src_files_over_500_lines",
        "prod_fns_over_100_lines",
        "engine_fs_outside_runtime",
        "numeric_const_without_reason",
        "doc_reference",
        "cli_word_outside_cli",
        "cli_print_outside_output",
        "spanish_prose_lines",
        "module_docs_over_60_lines",
        "mode_name_in_prod",
    ];

    /// Every counter a rise can name says how to bring it down.
    #[test]
    fn every_counter_says_how_to_bring_it_down() {
        for name in measure().keys() {
            assert_ne!(
                baseline::fix(name),
                baseline::UNNAMED_FIX,
                "`{name}` names no fix"
            );
        }
    }

    #[test]
    fn the_ratchet_measures_the_vocabulary_and_the_tense_the_repository_rules_on() {
        let counts = measure();
        for name in ["banned_vocabulary", "tense_markers"] {
            assert!(
                counts.contains_key(name),
                "`{name}` is a rule this repository states and the ratchet does not count, \
                 so a change is free to raise it"
            );
        }
    }

    #[test]
    fn the_baseline_holds_a_number_for_every_counter_the_ratchet_measures() {
        let counts = measure();
        let baseline = baseline::parse(
            &std::fs::read_to_string(baseline_path()).expect("the baseline is committed"),
        );
        let measured: Vec<&String> = counts.keys().collect();
        let held: Vec<&String> = baseline.keys().collect();
        assert_eq!(
            measured, held,
            "a counter enters with the number it measures when it lands — one with no line in \
             the baseline gates nothing, and a line with no counter gates a pattern nobody reads"
        );
    }

    #[test]
    fn the_ratchet_holds_doc_references_and_bare_numeric_consts_at_zero() {
        let counts = measure();
        assert_eq!(counts.get("doc_reference"), Some(&0));
        assert_eq!(counts.get("numeric_const_without_reason"), Some(&0));
    }

    #[test]
    fn the_tree_holds_each_counter_that_reaches_zero_at_zero() {
        let counts = measure();
        for name in ZERO_COUNTERS {
            assert_eq!(counts.get(*name), Some(&0), "{name}");
        }
    }

    #[test]
    #[should_panic(expected = "cannot read")]
    fn an_unreadable_corpus_file_stops_the_ratchet() {
        count_lines(&[PathBuf::from("/nonexistent/yunta-ratchet.rs")], |_| true);
    }
}
