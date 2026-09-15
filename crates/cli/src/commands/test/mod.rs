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
//! `paused` | `failed` | `promoted`), `nodes` and `tasks`.

mod case;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_adapters::{MockAdapter, MockFixture, RunPaths, MOCK_ID};

use crate::error::{CliError, Outcome};

pub(crate) use case::run_case;

pub async fn test(dir: Option<&Path>) -> Result<Outcome, CliError> {
    let root = project_root(dir).map_err(CliError::msg)?;

    let tests_dir = root.join(".yunta/tests");
    let Some(case_paths) = discover_case_paths(&root) else {
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

    let mut failures = 0usize;
    for case_path in &case_paths {
        let name = case_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| case_path.display().to_string());
        // A case that could not run at all is one problem like any
        // other: the verdict column says which kind of failure it was,
        // and one block under it counts and lists what went wrong.
        let (verdict, problems) = match run_case(&root, case_path).await {
            Ok(problems) if problems.is_empty() => {
                println!("case {name} ... ok");
                continue;
            }
            Ok(problems) => ("FAILED", problems),
            Err(error) => ("ERROR", vec![error.to_string()]),
        };
        failures += 1;
        println!(
            "{}",
            yunta_core::text::problems(format!("case {name} ... {verdict}"), &problems)
        );
    }

    println!(
        "{}, {failures} failed",
        yunta_core::text::counted(case_paths.len(), "case")
    );
    if failures == 0 {
        Ok(Outcome::Success)
    } else {
        Ok(Outcome::Reported)
    }
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

/// Reads a mock fixture and parses it against the run's own
/// directories — for `yunta test` and for `yunta run --adapter mock
/// --fixture` alike.
pub(crate) fn load_mock_fixture(
    fixture_path: &Path,
    run_dir: &Path,
    worktree: &Path,
) -> Result<Arc<MockAdapter>, CliError> {
    let text = std::fs::read_to_string(fixture_path)
        .map_err(|e| CliError::io("read fixture", fixture_path.display(), e))?;
    let paths = RunPaths {
        run_dir,
        worktree,
        staging: &yunta_engine::run_dir::staging_root(run_dir),
    };
    MockFixture::parse(&text, &paths)
        .map(|fixture| Arc::new(MockAdapter::new(fixture)))
        .map_err(|source| CliError::FixtureRefused {
            path: fixture_path.to_path_buf(),
            source,
        })
}

/// The mock standing in for every adapter the config names, so the
/// same workflow resolves the same runners with no LLM behind them.
pub(crate) fn mock_adapters(
    config: &yunta_core::ConfigLayer,
    mock: Arc<MockAdapter>,
) -> super::Adapters {
    let mut adapters = super::Adapters::new();
    adapters.insert(MOCK_ID.clone(), mock.clone());
    if let Some(runners) = &config.runners {
        for candidate in runners.values().flatten() {
            adapters
                .entry(candidate.adapter.clone())
                .or_insert_with(|| mock.clone());
        }
    }
    adapters
}

/// Copies `from`'s tree into `into`, which already exists. Every entry
/// is copied as a regular file or directory; the seed is repository
/// content a case commits, never a place for symlinks.
fn copy_dir_all(from: &Path, into: &Path) -> std::io::Result<()> {
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

/// Turns the sandbox worktree into a repository whose initial commit
/// holds the seed (or nothing), so a run's scope diff only ever shows
/// what its sessions changed.
fn init_git(dir: &Path) -> Result<(), CliError> {
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "yunta-test@localhost"],
        vec!["config", "user.name", "yunta test"],
        vec!["add", "-A"],
        vec!["commit", "-q", "--allow-empty", "-m", "sandbox"],
    ] {
        let ran = yunta_engine::git::success_blocking(dir, &args)
            .map_err(|e| CliError::msg(format!("git {args:?}: {}", e.detail())))?;
        if !ran {
            return Err(CliError::msg(format!(
                "git {args:?} failed in `{}`",
                dir.display()
            )));
        }
    }
    Ok(())
}
