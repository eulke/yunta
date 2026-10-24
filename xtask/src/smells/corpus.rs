//! How a measurement lists a directory and tells test code from production.
//! Every counter that walks the tree goes through here, so a directory it
//! cannot list stops the measure, and one rule says which files are tests.

use std::path::{Path, PathBuf};

use super::read_corpus;
use super::shape::production_only;

/// The entries of `dir`, in no particular order. A directory that does not
/// exist has none; any other failure stops the measure, because a measure
/// that cannot list a directory cannot report it clean.
pub(super) fn entries(dir: &Path) -> Vec<PathBuf> {
    let listing = match std::fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => panic!("the ratchet cannot list `{}`: {error}", dir.display()),
    };
    listing
        .map(|entry| match entry {
            Ok(entry) => entry.path(),
            Err(error) => panic!("the ratchet cannot list `{}`: {error}", dir.display()),
        })
        .collect()
}

/// Whether `path` is an asset no counter reads as text: an image, a PDF, or
/// the folder metadata a desktop leaves.
pub(super) fn is_asset(path: &Path) -> bool {
    let named = |name: &str| path.file_name().is_some_and(|file| file == name);
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase);
    named(".DS_Store")
        || extension.is_some_and(|ext| {
            ["png", "jpg", "jpeg", "gif", "webp", "ico", "pdf"].contains(&ext.as_str())
        })
}

/// Every `.rs` file under `dir`, recursively; missing directories yield
/// nothing so a counter over a path that does not exist is simply zero.
pub(super) fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs(dir, &mut out);
    out.sort();
    out
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for path in entries(dir) {
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The `src` directory of every crate under `crates/`.
pub(super) fn crate_src_dirs(root: &Path) -> Vec<PathBuf> {
    crate_subdirs(root, "src")
}

/// The `tests` directory of every crate under `crates/`.
pub(super) fn crate_test_dirs(root: &Path) -> Vec<PathBuf> {
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
    let mut dirs: Vec<PathBuf> = entries(&root.join("crates"))
        .into_iter()
        .map(|entry| entry.join(name))
        .filter(|sub| sub.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Whether `path` is a test module kept in a file of its own: a `tests.rs`
/// under a crate's `src`, declared by its parent as `#[cfg(test)] mod tests;`.
/// This is the one rule that tells test code from production, and every
/// counter that separates the two reads it here.
pub(super) fn is_test_source(path: &Path) -> bool {
    let shown = path.to_string_lossy().replace('\\', "/");
    let under_a_src = shown.split("/crates/").skip(1).any(|rest| {
        rest.split_once('/')
            .is_some_and(|(_, tail)| tail.starts_with("src/"))
    });
    under_a_src && shown.ends_with("/tests.rs")
}

/// The files the counters read, listed once for every counter of a measure,
/// so two counters never disagree about what they read.
pub(super) struct Corpus {
    /// Every `.rs` file under a crate's `src`.
    pub(super) src: Vec<PathBuf>,
    /// Every `.rs` file under a crate's `tests`.
    pub(super) tests: Vec<PathBuf>,
    /// The sources a person reads drawn on a terminal: the command line and
    /// the crate it draws with.
    pub(super) drawn: Vec<PathBuf>,
    /// The production text of each `src` file that is not test code, with
    /// its unit-test modules blanked.
    pub(super) prod: Vec<String>,
}

impl Corpus {
    /// Lists and reads the corpus under the workspace `root`.
    pub(super) fn of(root: &Path) -> Self {
        let src: Vec<PathBuf> = crate_src_dirs(root)
            .iter()
            .flat_map(|dir| rs_files(dir))
            .collect();
        let tests: Vec<PathBuf> = crate_test_dirs(root)
            .iter()
            .flat_map(|dir| rs_files(dir))
            .collect();
        let drawn: Vec<PathBuf> = ["crates/cli/src", "crates/render/src"]
            .iter()
            .flat_map(|dir| rs_files(&root.join(dir)))
            .collect();
        let prod: Vec<String> = src
            .iter()
            .filter(|path| !is_test_source(path))
            .map(|path| production_only(&read_corpus(path)))
            .collect();
        Self {
            src,
            tests,
            drawn,
            prod,
        }
    }
}

/// A directory under the system's temp directory holding `files`, each a path
/// relative to it with its text. The name keeps the trees of two tests apart.
#[cfg(test)]
pub(super) fn scratch_tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("yunta-smells-{name}-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("a stale scratch tree is removed");
    }
    for (path, text) in files {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().expect("a file has a directory"))
            .expect("a directory");
        std::fs::write(full, text).expect("a file");
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smells::count_numeric_const_without_reason;
    use crate::smells::disk::count_engine_fs_outside_runtime;

    #[test]
    fn a_tests_file_under_a_crate_src_is_test_code_and_nothing_else_is() {
        assert!(is_test_source(Path::new(
            "/w/crates/cli/src/commands/advice/tests.rs"
        )));
        assert!(!is_test_source(Path::new(
            "/w/crates/cli/src/commands/advice.rs"
        )));
        // A test directory holds test code by its place, not by a `tests.rs` name.
        assert!(!is_test_source(Path::new("/w/crates/cli/tests/tests.rs")));
        assert!(!is_test_source(Path::new("/w/xtask/src/tests.rs")));
    }

    #[test]
    fn a_tests_file_under_a_crate_src_is_counted_by_neither_bare_consts_nor_disk_calls() {
        let root = scratch_tree(
            "tests-file-exempt",
            &[
                (
                    "crates/engine/src/limits.rs",
                    "const ATTEMPTS: u32 = 3;\nfn load() {\n    let text = std::fs::read_to_string(\"a\");\n}\n",
                ),
                (
                    "crates/engine/src/tests.rs",
                    "const RETRIES: u32 = 5;\nfn load() {\n    let text = std::fs::read_to_string(\"b\");\n}\n",
                ),
            ],
        );
        assert_eq!(count_numeric_const_without_reason(&root), 1);
        assert_eq!(count_engine_fs_outside_runtime(&root), 1);
    }
}
