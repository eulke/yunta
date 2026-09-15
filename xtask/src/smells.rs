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
//! that own a capability ([`sites`]), the shape of a test, and a
//! threshold with no decision behind it. A rule nothing measures is a
//! rule a change is free to break.
//!
//! Counting is line-based (a line matching the pattern), the same measure
//! the rebuild's own acceptance commands used, so a contributor and CI see
//! the same number `grep -c` would.

mod prose;
mod shape;
mod sites;
mod thresholds;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use shape::{functions_over, has_inner_space_run, production_only, strip_noise, sync_fs_in_async};
use thresholds::numeric_consts_without_decision;

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

/// Every `.rs` file under `dir`, recursively; missing directories yield
/// nothing so a counter over a path that does not exist is simply zero.
fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs(dir, &mut out);
    out.sort();
    out
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The `src` directory of every crate under `crates/`.
fn crate_src_dirs(root: &Path) -> Vec<PathBuf> {
    crate_subdirs(root, "src")
}

/// The `tests` directory of every crate under `crates/`.
fn crate_test_dirs(root: &Path) -> Vec<PathBuf> {
    crate_subdirs(root, "tests")
}

/// The files the ratchet reads about code: the sources of every crate and
/// the tests that drive them. One corpus, so two counters naming the same
/// pattern never disagree about where they looked.
pub(super) fn ratchet_corpus(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = crate_src_dirs(root)
        .iter()
        .chain(crate_test_dirs(root).iter())
        .flat_map(|dir| rs_files(dir))
        .collect();
    files.sort();
    files
}

fn crate_subdirs(root: &Path, name: &str) -> Vec<PathBuf> {
    let crates = root.join("crates");
    let mut dirs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&crates) {
        for entry in entries.flatten() {
            let sub = entry.path().join(name);
            if sub.is_dir() {
                dirs.push(sub);
            }
        }
    }
    dirs.sort();
    dirs
}

/// Lines across `files` that satisfy `matches`.
fn count_lines(files: &[PathBuf], matches: impl Fn(&str) -> bool) -> usize {
    files
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.lines().filter(|line| matches(line)).count())
        .sum()
}

/// Every counter, by name, computed fresh from the tree.
fn measure() -> BTreeMap<String, usize> {
    let root = workspace_root();
    let src_files: Vec<PathBuf> = crate_src_dirs(&root)
        .iter()
        .flat_map(|dir| rs_files(dir))
        .collect();
    let tests: Vec<PathBuf> = crate_test_dirs(&root)
        .iter()
        .flat_map(|dir| rs_files(dir))
        .collect();
    let cli_src = rs_files(&root.join("crates/cli/src"));

    // Production text (unit-test modules blanked) for the counters the audit
    // measured outside tests.
    let prod: Vec<String> = src_files
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| production_only(&text))
        .collect();

    let mut counts = BTreeMap::new();

    // Production `unwrap`/`expect`/`panic`/indexing are not counted here:
    // the crate-root `#![deny(clippy::…)]` lints make each a compile error,
    // which CI's `cargo clippy -- -D warnings` already enforces — a stronger
    // gate than a ratchet, and one a comment mention of `.unwrap()` can't
    // inflate.

    // A discarded `Result` hides a degradation the log should carry.
    counts.insert(
        "let_underscore_in_prod".to_string(),
        prod.iter()
            .flat_map(|text| text.lines())
            .filter(|line| line.trim_start().starts_with("let _ ="))
            .count(),
    );
    // A production function near 50 lines is a seam not yet cut.
    counts.insert(
        "prod_fns_over_50_lines".to_string(),
        prod.iter().map(|text| functions_over(text, 50)).sum(),
    );
    // A file near 500 lines is another (counted whole — its tests are part
    // of what a reader scrolls).
    counts.insert(
        "src_files_over_500_lines".to_string(),
        src_files
            .iter()
            .filter(|path| {
                std::fs::read_to_string(path)
                    .map(|text| text.lines().count() > 500)
                    .unwrap_or(false)
            })
            .count(),
    );
    // The CLI has one error translator; `main` maps the exit code.
    counts.insert(
        "exit_failure_in_cli".to_string(),
        count_lines(&cli_src, |line| line.contains("ExitCode::FAILURE")),
    );
    // Test infrastructure lives in the support crate, never copied into a
    // test that needs it. Counted over `src` as well as `tests`, because
    // a `#[cfg(test)]` module inside a crate is a test that needs it too,
    // and a copy there drifts from the one place just as quietly — the
    // one this found had dropped the initial-branch pin that makes the
    // support crate's version hermetic.
    let test_code: Vec<PathBuf> = tests.iter().chain(src_files.iter()).cloned().collect();
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

    // A `_ =>` inside a ledger's fold is a kind that derives nothing
    // with nothing saying so: the arm exists, the reader sees no
    // diagnostic, and the state is simply wrong. Every kind is named,
    // and one that moves nothing says `is_audit`.
    counts.insert(
        "wildcard_in_ledger_apply".to_string(),
        wildcards_in_folds(&root),
    );

    // A word this repository retired still naming the thing it retired it
    // for, and a text that describes a plan or a past instead of what the
    // repository does. Both read prose, over a corpus wider than `.rs`.
    counts.insert(
        "banned_vocabulary".to_string(),
        prose::banned_vocabulary(&root),
    );
    counts.insert("tense_markers".to_string(), prose::tense_markers(&root));

    // A run of spaces inside a message is a `\` continuation that went
    // missing: the reader gets the source's indentation in the text.
    counts.insert(
        "space_runs_in_prod_strings".to_string(),
        prod.iter()
            .flat_map(|text| text.lines())
            .filter(|line| has_inner_space_run(line))
            .count(),
    );

    // The shape of a test, the thread an async body holds, the threshold
    // with no decision behind it, and every capability written outside the
    // site that owns it. Each of these is a function of the tree alone.
    for (name, count) in COUNTERS.iter().chain(sites::COUNTERS) {
        counts.insert((*name).to_string(), count(&root));
    }

    counts
}

/// The counters that read the corpus of code and nothing else, beside the
/// ones [`sites`] declares.
const COUNTERS: &[sites::Counter] = &[
    ("sync_fs_in_async", count_sync_fs_in_async),
    ("test_files_over_500_lines", count_test_files_over_500_lines),
    ("test_fns_over_50_lines", count_test_fns_over_50_lines),
    ("numeric_const_without_adr", count_numeric_const_without_adr),
];

/// A synchronous file call inside an async body: it holds the thread the
/// runtime gave the task until the disk answers.
fn count_sync_fs_in_async(root: &Path) -> usize {
    ratchet_corpus(root)
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| sync_fs_in_async(&text))
        .sum()
}

/// A test file past the budget a source file has. What a reader scrolls
/// to understand a failure is the test as well as the code it drives.
fn count_test_files_over_500_lines(root: &Path) -> usize {
    test_files(root)
        .iter()
        .filter(|path| {
            std::fs::read_to_string(path)
                .map(|text| text.lines().count() > 500)
                .unwrap_or(false)
        })
        .count()
}

/// A test function past the budget a production function has. A test that
/// long names more than one behaviour, and a failure in it says which
/// line broke rather than which promise.
fn count_test_fns_over_50_lines(root: &Path) -> usize {
    test_files(root)
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| functions_over(&text, 50))
        .sum()
}

/// A threshold with no decision behind it, over `src`, which is where
/// D170 states the rule: a number a test writes is read beside the
/// assertion it serves, while a number the code carries governs what the
/// binary does to somebody's run.
fn count_numeric_const_without_adr(root: &Path) -> usize {
    crate_src_dirs(root)
        .iter()
        .flat_map(|dir| rs_files(dir))
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| numeric_consts_without_decision(&text))
        .sum()
}

fn test_files(root: &Path) -> Vec<PathBuf> {
    crate_test_dirs(root)
        .iter()
        .flat_map(|dir| rs_files(dir))
        .collect()
}

fn render(counts: &BTreeMap<String, usize>) -> String {
    counts
        .iter()
        .map(|(name, count)| format!("{name} {count}\n"))
        .collect()
}

fn parse_baseline(text: &str) -> BTreeMap<String, usize> {
    text.lines()
        .filter_map(|line| {
            let (name, count) = line.rsplit_once(' ')?;
            Some((name.to_string(), count.trim().parse().ok()?))
        })
        .collect()
}

/// Wildcard match arms inside the functions that fold the log: every
/// `ledger.rs` under `crates/core/src/events/`, plus the engine's own
/// derivation. A `_ =>` there is a kind that derives nothing with
/// nothing saying so.
fn wildcards_in_folds(root: &Path) -> usize {
    let mut folds: Vec<PathBuf> = Vec::new();
    if let Ok(domains) = std::fs::read_dir(root.join("crates/core/src/events")) {
        for domain in domains.flatten() {
            let ledger = domain.path().join("ledger.rs");
            if ledger.is_file() {
                folds.push(ledger);
            }
        }
    }
    folds.push(root.join("crates/engine/src/replay.rs"));
    folds.sort();
    folds
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| wildcards_in_apply(&text))
        .sum()
}

/// Wildcard arms inside a fold's `apply`, and nowhere else in the file:
/// a `match` over an `Option` or a pair of them is ordinary reading, and
/// what this measures is a kind of event nothing names.
fn wildcards_in_apply(source: &str) -> usize {
    let mut count = 0;
    let mut depth = 0usize;
    let mut inside = false;
    let mut block = false;
    for line in source.lines() {
        let (code, next) = strip_noise(line, block);
        block = next;
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

/// `cargo xtask smells`: measure and write the baseline.
pub fn write() -> Result<(), String> {
    let counts = measure();
    let path = baseline_path();
    std::fs::write(&path, render(&counts))
        .map_err(|e| format!("cannot write `{}`: {e}", path.display()))?;
    print!("{}", render(&counts));
    println!("wrote {}", path.display());
    Ok(())
}

/// `cargo xtask smells --check`: fail if any count rose above the baseline,
/// or if the baseline is missing a counter the tree now measures.
pub fn check() -> Result<(), String> {
    let counts = measure();
    let path = baseline_path();
    let baseline = parse_baseline(&std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "cannot read `{}` — run `cargo xtask smells`: {e}",
            path.display()
        )
    })?);

    let mut risen = Vec::new();
    for (name, &count) in &counts {
        match baseline.get(name) {
            Some(&allowed) if count <= allowed => {}
            Some(&allowed) => risen.push(format!("  {name}: {allowed} -> {count}")),
            None => risen.push(format!("  {name}: (new) -> {count}")),
        }
    }
    if risen.is_empty() {
        println!("smells: no count rose above the baseline");
        return Ok(());
    }
    Err(format!(
        "these smell counts rose above `{}` — fix them, or lower the baseline in the same \
         change if the rise is deliberate:\n{}",
        path.display(),
        risen.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let baseline = parse_baseline(
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
}
