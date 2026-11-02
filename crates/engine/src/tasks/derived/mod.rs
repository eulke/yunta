//! What a plan's shapes are to the tree the run works in, read once when
//! the plan is registered: what a task may write beyond the scope it
//! declared because a shape it owns is named there, and what the tree
//! held of each shape when the plan was written.
//!
//! A task that changes a shape changes what names it: the caller of a
//! signature it widens, the match on an enum it grows. The plan declares
//! the shape and its owner; which files name it is a fact of the tree the
//! run works in, so the engine reads it there — rather than asking the
//! planner to foresee every caller, or a person to allow each one once
//! the build points at it. Only code names it: a fixture string that
//! quotes a plan, or a comment, calls nothing, and reaching for it would
//! only keep the owner from working beside the task whose file it is.
//!
//! One search finds every file that carries a shape's name, and one batch
//! reads them; that [`Reading`] answers both questions. A file git would
//! call binary, a NUL among its first bytes, is no code whatever its
//! extension says: it names no shape, so it is never reached for and never
//! implied, and a shape planned in it is new. [`reach`] states what each
//! task may write, and [`shapes`] what the tree held of each shape, which
//! every page that draws the plan reads.

mod reach;
mod shapes;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use surco::code::Language;
use yunta_core::{CommitSha, NodeId, Shape, TasksFile};

use super::code_words;
use crate::process::Supervision;
use crate::replay::RunView;
use crate::run::RunError;
use crate::run_log::RunLog;

/// A name more files of its language carry than this is a word of the
/// language more than a shape of the plan: following it would let its owner
/// write half the tree, and listing its files would tell a person nothing.
const COMMON: usize = 20;

/// How far into a file git looks for a NUL before it calls the file
/// binary: the reading tells code from bytes the way git would.
const BINARY_HEAD: usize = 8000;

/// Reads `tree` at its HEAD for the shapes of `document`, and states what
/// the reading says: each task's reach, once per change — a task whose
/// reach the log already states as this one is left alone, and one a recut
/// gave other shapes is stated again, replacing it — and what the tree
/// held of the plan's shapes, once for each plan and commit. A plan with no
/// shape to follow and no reach to restate reads nothing.
pub(crate) async fn derive_from_tree(
    log: &RunLog<'_>,
    node: Option<&NodeId>,
    document: &TasksFile,
    tree: &Path,
    supervision: Supervision<'_>,
) -> Result<(), RunError> {
    let view = RunView::of(log.events().await?);
    let followed = followed(document);
    let restated = document
        .tasks
        .iter()
        .any(|task| view.state.grants.derived_for(&task.id).is_some());
    if followed.is_empty() && !restated {
        return Ok(());
    }
    let at = crate::worktree::head_commit(tree, supervision).await?;
    let reading = Reading::of(&followed, tree, at, supervision).await?;
    reach::state(log, node, document, &reading, &view.state.grants).await?;
    shapes::state(log, node, document, &reading, &view.state).await
}

/// What the tree held, at one commit, of the names a plan's shapes are
/// called by, and the text of every file read for them.
struct Reading {
    at: CommitSha,
    /// Who names each followed shape, by the shape's name.
    named: BTreeMap<String, Named>,
    /// The text of each file read, by its path in the repository.
    sources: BTreeMap<String, String>,
}

/// Who names one shape's name, as the reading found it.
enum Named {
    /// Each file of its language whose code names it, its own among them
    /// when it does.
    Files(BTreeSet<String>),
    /// How many files of its language carry it — more than are followed —
    /// and whether its own file's code names it.
    Common { files: usize, own: bool },
}

impl Reading {
    /// What `tree` holds at `at` of the names of `followed`: one search for
    /// every name among the files of their languages, and one batch that
    /// reads each file a shape is followed into.
    async fn of(
        followed: &[&Shape],
        tree: &Path,
        at: CommitSha,
        supervision: Supervision<'_>,
    ) -> Result<Reading, RunError> {
        let words: Vec<&str> = followed.iter().map(|shape| shape.name.as_str()).collect();
        let found =
            crate::git::words_in(tree, at.as_str(), &words, &globs(followed), supervision).await?;
        let listed: Vec<(&Shape, BTreeSet<String>)> = followed
            .iter()
            .map(|shape| (*shape, of_its_language(shape, &found)))
            .collect();
        let sources = crate::git::blobs(tree, at.as_str(), &wanted(&listed), supervision).await?;
        let named = listed
            .into_iter()
            .map(|(shape, files)| (shape.name.clone(), named(shape, files, &sources)))
            .collect();
        Ok(Reading { at, named, sources })
    }

    /// Who names the shape called `name`, when the reading followed it.
    fn named(&self, name: &str) -> Option<&Named> {
        self.named.get(name)
    }

    /// The text of the file at `path` in the repository, when the reading
    /// read it.
    fn source(&self, path: &str) -> Option<&str> {
        self.sources.get(path).map(String::as_str)
    }
}

impl Named {
    /// Whether the code of `own`, the file the shape lives in, names it.
    fn in_own(&self, own: &str) -> bool {
        match self {
            Named::Files(files) => files.contains(own),
            Named::Common { own: named, .. } => *named,
        }
    }
}

/// The shapes of `document` a reading follows into the tree: each whose
/// name is one word of code, in a file whose extension says its language.
fn followed(document: &TasksFile) -> Vec<&Shape> {
    document
        .shapes
        .iter()
        .filter(|shape| identifier(&shape.name) && extension(&shape.file).is_some())
        .collect()
}

/// A pathspec for each extension of the languages `followed` are written
/// in, each once.
fn globs(followed: &[&Shape]) -> Vec<String> {
    let extensions: BTreeSet<&str> = followed
        .iter()
        .filter_map(|shape| extension(&shape.file))
        .flat_map(Language::family)
        .collect();
    extensions
        .into_iter()
        .map(|kind| format!("*.{kind}"))
        .collect()
}

/// The files `found` lists for `shape`'s name that are written in its
/// file's language.
fn of_its_language(shape: &Shape, found: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    let family = extension(&shape.file).map_or_else(Vec::new, Language::family);
    found
        .get(&shape.name)
        .into_iter()
        .flatten()
        .filter(|file| extension(file).is_some_and(|kind| family.contains(&kind)))
        .cloned()
        .collect()
}

/// The files a reading reads: each file a shape is followed into, and of a
/// name too common to follow, only its own file.
fn wanted(listed: &[(&Shape, BTreeSet<String>)]) -> Vec<String> {
    let mut wanted = BTreeSet::new();
    for (shape, files) in listed {
        if files.len() <= COMMON {
            wanted.extend(files.iter().cloned());
            continue;
        }
        let own = yunta_core::in_repo(&shape.file);
        if files.contains(&own) {
            wanted.insert(own);
        }
    }
    wanted.into_iter().collect()
}

/// Who names `shape` among the `files` of its language that carry its
/// name, each read in `sources`: the files whose code names it, or, for a
/// name too common to follow, how many carry it and whether its own file's
/// code names it. A binary file's bytes are no code.
fn named(shape: &Shape, files: BTreeSet<String>, sources: &BTreeMap<String, String>) -> Named {
    let in_code = |file: &String| match (sources.get(file), extension(file)) {
        (Some(source), Some(kind)) => {
            !binary(source) && code_words::names_in_code(source, kind, &shape.name)
        }
        _ => false,
    };
    if files.len() > COMMON {
        return Named::Common {
            files: files.len(),
            own: in_code(&yunta_core::in_repo(&shape.file)),
        };
    }
    Named::Files(files.into_iter().filter(in_code).collect())
}

/// Whether `source` is a binary file's, as git tells one: a NUL among its
/// first bytes.
fn binary(source: &str) -> bool {
    source.bytes().take(BINARY_HEAD).any(|byte| byte == 0)
}

/// The extension of `file`, which says the language it is written in.
fn extension(file: &str) -> Option<&str> {
    Path::new(file).extension().and_then(|kind| kind.to_str())
}

/// Whether `name` is one word of code, as `git grep -w` reads a word: what
/// a shape the code calls by name is, and a file format the plan describes
/// is not.
fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::identifier;

    #[test]
    fn only_a_word_of_code_is_followed() {
        assert!(identifier("build_manifest"));
        assert!(identifier("PackScope"));
        assert!(!identifier("Bench::run"));
        assert!(!identifier("pack.lock format"));
        assert!(!identifier("2fa"));
    }
}
