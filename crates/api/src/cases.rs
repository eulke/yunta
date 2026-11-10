//! See [`crate`]. A project's cases: discovered under a directory's
//! `.yunta/tests/` (a repository's own, or a pack's), each workflow executed
//! with the `mock` adapter driven by the case's fixture, its final state
//! derived by replay and compared against `expect`. No LLM, no network.
//!
//! What a case fixes is what the run *does*: every session is scripted,
//! nobody is asked anything a `decisions:` entry does not answer, and the
//! same case run twice reaches the same nodes, tasks and final state. What
//! it does not fix is *when*: the run is stamped by the workspace's own clock
//! and its ids are minted from the workspace's own source, exactly as a run
//! is stamped and minted. Nothing a case asserts reads either.
//!
//! A case takes the same path a run takes, through the same functions: the
//! workflow resolves through the project's own catalog, `check` refuses
//! here whatever it would refuse there, the manifest is frozen the same way
//! and the run is driven by the same call. Its sandbox is a checkout of its
//! own (`crate::sandbox`): the project's `.yunta/` copied in, the case's
//! `worktree:` seed on top, a git repository around both, and a fresh runs
//! root and event-log DB beside it under a temp dir, so a test run never
//! touches the project's real state and a workflow that reads files, takes a
//! `path` input or runs the project's own toolchain has what it needs before
//! any session starts. The fixture is rendered with `{{run.dir}}` and
//! `{{run.worktree}}` before parsing, so a scripted session can place
//! artifacts exactly where a real agent (told `{{run.dir}}` in its prompt)
//! would.
//!
//! A case names the `workflow`, the `mode` it runs in (absent: the whole
//! graph), the `inputs` it provides (absent: each input's own default), the
//! `worktree` seed (absent: an empty repository), the `fixture`, the
//! `decisions` it answers each gate with (absent: the run parks on the first
//! one), and an `expect` block with `final_state` (`finished` | `needs you` |
//! `waiting` | `stopped` | `reported` | `failed` | `promoted`), `nodes` and
//! `tasks`
//! (`case`).

mod case;

use std::path::{Path, PathBuf};

use yunta_core::refusal::CasesRefusal;
use yunta_render::page::pack::Cases;
use yunta_render::page::{CaseProblem, CaseRun, RanCase, Tested};
use yunta_render::refusal::Refused;

use crate::remark::Unestimated;
use crate::{Error, Remarks, Stop, Workspace};

impl Workspace {
    /// The cases under `root`'s `.yunta/tests/`, in the order they run:
    /// refused when that directory does not read, or holds none.
    pub fn cases(&self, root: &Path) -> Result<CaseSuite<'_>, Error> {
        let dir = root.join(".yunta/tests");
        let Some(paths) = case_paths(root) else {
            return Err(Refused::Cases(CasesRefusal::Unreadable { dir }).into());
        };
        if paths.is_empty() {
            return Err(Refused::Cases(CasesRefusal::NoneFound { dir }).into());
        }
        Ok(CaseSuite {
            workspace: self,
            root: root.to_path_buf(),
            paths,
        })
    }

    /// A pack's own cases at `dir`, each run on it the way a project's own
    /// are run: none shipped where it has no `.yunta/tests/`, or an empty
    /// one.
    pub(crate) async fn cases_run(&self, dir: &Path, remarks: &dyn Remarks, stop: &Stop) -> Cases {
        let paths = case_paths(dir).unwrap_or_default();
        if paths.is_empty() {
            return Cases::NoneShipped;
        }
        let mut ran = Vec::new();
        for path in &paths {
            ran.push(run_one(self, dir, path, remarks, stop).await);
        }
        Cases::Ran(ran)
    }
}

/// A pack's own cases at `dir`, counted, none of them run: what an install
/// reports when nothing of the pack is to run.
pub(crate) fn shipped(dir: &Path) -> Cases {
    match case_paths(dir).map_or(0, |paths| paths.len()) {
        0 => Cases::NoneShipped,
        shipped => Cases::NotRun { shipped },
    }
}

/// A project's cases, found where they were looked for, run in turn.
pub struct CaseSuite<'w> {
    workspace: &'w Workspace,
    /// The directory whose `.yunta/` holds them: each case resolves its
    /// project there.
    root: PathBuf,
    paths: Vec<PathBuf>,
}

impl CaseSuite<'_> {
    /// What each case is called, in the order they run: its file's stem.
    pub fn names(&self) -> Vec<String> {
        self.paths.iter().map(|path| case_name(path)).collect()
    }

    /// Runs each case in turn, each in a sandbox of its own, and hands each
    /// to `each` as it ends; answers how many ran, and how many did not
    /// hold. What their runs say while they are made is said through
    /// `remarks`, but the estimate. The caller's stop stops the case that is
    /// running, and the ones after it stop as they start.
    pub async fn run(
        &self,
        mut each: impl FnMut(RanCase),
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Tested {
        let mut failures = 0;
        for path in &self.paths {
            let case = run_one(self.workspace, &self.root, path, remarks, stop).await;
            if !case.run.held() {
                failures += 1;
            }
            each(case);
        }
        Tested {
            cases: self.paths.len(),
            failures,
        }
    }
}

/// Runs the case at `path`, found under `root`, and says how it ended.
async fn run_one(
    workspace: &Workspace,
    root: &Path,
    path: &Path,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> RanCase {
    let said = Unestimated(remarks);
    let ran = case::run_case(workspace, root, path, &said, stop).await;
    RanCase {
        name: case_name(path),
        run: case_run(ran),
    }
}

/// How a case's run ended. A case that could not run at all is one problem
/// like any other, said in the words of what stopped it.
fn case_run(ran: Result<Vec<CaseProblem>, Error>) -> CaseRun {
    match ran {
        Ok(problems) if problems.is_empty() => CaseRun::Held,
        Ok(problems) => CaseRun::Failed(problems),
        Err(error) => CaseRun::CouldNotRun(error.to_string()),
    }
}

/// Every `.yaml`/`.yml` case file directly under `<root>/.yunta/tests/`,
/// sorted so the cases run in one order: `None` when that directory itself
/// does not exist (as opposed to existing and being empty, which is a
/// separate case each caller decides how to treat). A project's cases and a
/// pack's own are found by the one rule, so a pack's tests are authored
/// exactly like a repository's.
fn case_paths(root: &Path) -> Option<Vec<PathBuf>> {
    let tests_dir = root.join(".yunta/tests");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&tests_dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
        .collect();
    paths.sort();
    Some(paths)
}

/// What a case is called: its file's stem.
fn case_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
