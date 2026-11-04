//! `yunta test`: discover cases under a project root's `.yunta/tests/`
//! (the current directory, or `--dir`), execute each workflow with the
//! `mock` adapter driven by the case's fixture, derive the final state
//! by replay and compare it against `expect`. No LLM, no network.
//!
//! What a case fixes is what the run *does*: every session is scripted,
//! nobody is asked anything a `decisions:` entry does not answer, and
//! the same case run twice reaches the same nodes, tasks and final
//! state. What it does not fix is *when*: the run is stamped by the
//! invocation's own clock and its ids are minted from the invocation's
//! own source, exactly as `yunta run` stamps and mints. Nothing a case
//! asserts reads either.
//!
//! A case takes the same path a run takes, through the same functions:
//! the workflow resolves through the project's own catalog, `check`
//! refuses here whatever it would refuse there, the manifest is frozen
//! the same way and the run is driven by the same call. Its sandbox is
//! a checkout of its own — this project's `.yunta/` copied in, the
//! case's `worktree:` seed on top, a git repository around both, and a
//! fresh runs root and event-log DB beside it under a temp dir — so a
//! test run never touches the project's real state and a workflow that
//! reads files, takes a `path` input or runs the project's own
//! toolchain has what it needs before any session starts. The fixture
//! file is rendered with `{{run.dir}}` and `{{run.worktree}}` before
//! parsing, so a scripted session can place artifacts exactly where a
//! real agent (told `{{run.dir}}` in its prompt) would.
//!
//! A case names the `workflow`, the `mode` it runs in (absent: the whole
//! graph), the `inputs` it provides (absent: each input's own default),
//! the `worktree` seed (absent: an empty repository), the `fixture`, the
//! `decisions` it answers each gate with (absent: the run parks on the
//! first one), and an `expect` block with `final_state` (`finished` |
//! `needs you` | `waiting` | `reported` | `failed` | `promoted`), `nodes` and
//! `tasks`.

mod case;

use std::path::{Path, PathBuf};

use crate::error::{CliError, Outcome};
use crate::output::document;
use surco::components::{Check, Checklist, Found, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;

pub(crate) use case::run_case;

pub async fn test(dir: Option<&Path>, look: &Look) -> Result<Outcome, CliError> {
    let root = project_root(dir).map_err(CliError::msg)?;
    // One interruption for the whole suite: every case's sandbox shares
    // its two stages, so a Ctrl-C stops the case that is running and
    // the ones after it never start.
    let interrupt = crate::interrupt::Interrupt::ctrl_c()
        .map_err(|source| CliError::io("install the interrupt handler for", "test", source))?;

    let case_paths = cases_under(&root)?;

    // Every case's name is known before the first one runs, so the
    // verdicts line up in one column as each case finishes.
    let names: Vec<String> = case_paths.iter().map(|path| case_name(path)).collect();
    let mut failures = 0usize;
    for case_path in &case_paths {
        if !report(&root, case_path, interrupt.shared(), (&names, look)).await {
            failures += 1;
        }
    }

    verdict(case_paths.len(), failures, look);
    if failures == 0 {
        Ok(Outcome::Success)
    } else {
        Ok(Outcome::Reported)
    }
}

/// Every case under `root`'s `.yunta/tests`, refused when there is none
/// to run or the directory does not read.
fn cases_under(root: &Path) -> Result<Vec<std::path::PathBuf>, CliError> {
    let tests_dir = root.join(".yunta/tests");
    let Some(case_paths) = discover_case_paths(root) else {
        return Err(CliError::msg(format!(
            "cannot read test cases from {}",
            tests_dir.display()
        )));
    };
    if case_paths.is_empty() {
        return Err(CliError::msg(format!(
            "no test cases found under {}",
            tests_dir.display()
        )));
    }
    Ok(case_paths)
}

/// The verdict under the rows it counts, as a run's closing comes under
/// what it showed while it ran.
fn verdict(cases: usize, failures: usize, look: &Look) {
    let tally = format!("{}, {failures} failed", surco::text::counted(cases, "case"));
    let mark = match failures {
        0 => Mark::Done,
        _ => Mark::Failed,
    };
    crate::output::blank();
    document(
        look,
        Doc::new().with(Headline::new(mark, super::advice::test(), tally)),
    );
}

/// The project whose `.yunta/` holds the cases: `dir` when given, the
/// current directory otherwise. Always absolute, so every case's sandbox
/// and fixture resolve from one fixed root regardless of where the
/// command was invoked.
fn project_root(dir: Option<&Path>) -> Result<PathBuf, String> {
    let root = match dir {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|e| format!("cannot determine the current directory: {e}"))?,
    };
    std::path::absolute(&root).map_err(|e| format!("cannot resolve `{}`: {e}", root.display()))
}

/// Every `.yaml`/`.yml` case file directly under `<root>/.yunta/tests/`,
/// sorted for reproducible ordering — `None` when that directory itself
/// doesn't exist (as opposed to existing and being empty, which is a
/// separate case each caller decides how to treat). Shared by `yunta
/// test` (root = the project's own `cwd`) and `yunta pack audit`
/// (root = a pack directory) — same case format, same discovery rule,
/// so a pack's own tests are authored exactly like a repo's.
pub(crate) fn discover_case_paths(root: &Path) -> Option<Vec<PathBuf>> {
    let tests_dir = root.join(".yunta/tests");
    let mut case_paths: Vec<PathBuf> = std::fs::read_dir(&tests_dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
        .collect();
    case_paths.sort();
    Some(case_paths)
}

/// A checkout of its own for a run that must not touch the project: a
/// temp root, and under it a worktree seeded with this project's
/// catalog. Whoever built it writes what the run needs on top and then
/// makes it a repository with [`init_git`].
pub(crate) struct SandboxedCheckout {
    /// Owns the temp root, so the sandbox lasts exactly as long as this
    /// value and not a moment longer.
    root: tempfile::TempDir,
    worktree: PathBuf,
}

impl SandboxedCheckout {
    /// Where the run's state — its event log, its run directories —
    /// goes, so the project's own is never written to.
    pub(crate) fn root(&self) -> &Path {
        self.root.path()
    }

    /// The tree the run works in.
    pub(crate) fn worktree(&self) -> &Path {
        &self.worktree
    }
}

/// Builds one, seeded from `cwd`'s `.yunta`, so a run inside it
/// resolves its workflow through the very catalog a real run resolves
/// through.
pub(crate) fn sandboxed_checkout(cwd: &Path) -> Result<SandboxedCheckout, CliError> {
    let root = tempfile::tempdir().map_err(|e| CliError::io("create", "a sandbox", e))?;
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree)
        .map_err(|e| CliError::io("create", worktree.display(), e))?;
    let catalog = cwd.join(".yunta");
    if catalog.is_dir() {
        // Created here rather than left to the first entry that happens
        // to be a directory: a catalog holding only files would
        // otherwise be copied into a directory that is not there.
        let into = worktree.join(".yunta");
        std::fs::create_dir_all(&into).map_err(|e| CliError::io("create", into.display(), e))?;
        copy_dir_all(&catalog, &into)
            .map_err(|e| CliError::io("copy the catalog from", catalog.display(), e))?;
    }
    Ok(SandboxedCheckout { root, worktree })
}

/// Copies `from`'s tree into `into`, which already exists. Every entry
/// is copied as a regular file or directory; the seed is repository
/// content a case commits, never a place for symlinks.
pub(crate) fn copy_dir_all(from: &Path, into: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = into.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// What a case is called: its file's stem.
fn case_name(case_path: &Path) -> String {
    case_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| case_path.display().to_string())
}

/// Runs one case and prints its row — its mark, its name in the column
/// every one of `names` fits, and what it found — with each problem under
/// it.
/// `true` when it passed.
///
/// A case that could not run at all is one problem like any other: the
/// row says which kind of failure it was, and the lines under it list
/// what went wrong.
async fn report(
    root: &Path,
    case_path: &Path,
    interrupt: crate::interrupt::Interrupt,
    (names, look): (&[String], &surco::look::Look),
) -> bool {
    let name = case_name(case_path);
    let (found, said, problems) = case_found(run_case(root, case_path, interrupt).await);
    let passed = problems.is_empty();
    // The cases run one at a time and each row is drawn as its case ends:
    // the column every row shares is the list's, not the row's.
    let mut row = Checklist::aligned(names.to_vec());
    row.push_check(Check::new(found, name, said).details(problems));
    document(look, Doc::new().with(row));
    passed
}

/// What a case's run found, as its row says it: whether it held, what its
/// row says, and each problem, as written. A case that could not run at all
/// is one problem like any other.
pub(crate) fn case_found(ran: Result<Vec<String>, CliError>) -> (Found, String, Vec<String>) {
    match ran {
        Ok(problems) if problems.is_empty() => (Found::Holds, "ok".to_string(), Vec::new()),
        Ok(problems) => (
            Found::Problem,
            format!("failed: {}", surco::text::counted(problems.len(), "error")),
            problems,
        ),
        Err(error) => (
            Found::Problem,
            "could not run: 1 error".to_string(),
            vec![error.to_string()],
        ),
    }
}

/// Turns the sandbox worktree into a repository whose initial commit
/// holds the seed (or nothing), so a run's scope diff only ever shows
/// what its sessions changed.
pub(crate) async fn init_git(
    dir: &Path,
    supervision: yunta_engine::process::Supervision<'_>,
) -> Result<(), CliError> {
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "yunta-test@localhost"],
        vec!["config", "user.name", "yunta-test"],
        vec!["add", "-A"],
        vec!["commit", "-q", "--allow-empty", "-m", "sandbox"],
    ] {
        yunta_engine::git::change(dir, yunta_engine::git::Effect::Checkout, &args, supervision)
            .await
            .map_err(yunta_engine::RunError::from)?;
    }
    Ok(())
}
