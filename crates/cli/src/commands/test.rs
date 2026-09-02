//! `yunta test`: discover cases under a project root's `.yunta/tests/`
//! (the current directory, or `--dir`), execute each workflow with the
//! `mock` adapter driven by the case's fixture, derive the final state
//! by replay and compare it against `expect`. No LLM, no network,
//! deterministic.
//!
//! Each case runs in its own sandbox: a fresh git worktree, a fresh runs
//! root and a fresh event-log DB under a temp dir — a test run never
//! touches the project's real state. The worktree starts empty, or as a
//! copy of the case's `worktree:` directory committed as the sandbox's
//! initial commit, so a workflow that reads files, takes a `path` input
//! or runs the project's own toolchain has what it needs before any
//! session starts. The fixture file is rendered with `{{run.dir}}` and
//! `{{worktree}}` before parsing, so a scripted session can place
//! artifacts exactly where a real agent (told `{{run.dir}}` in its
//! prompt) would.
//!
//! A case names the `workflow`, the `mode` it runs in (absent: the whole
//! graph), the `inputs` it provides (absent: each input's own default),
//! the `worktree` seed (absent: an empty repository), the `fixture`, and
//! an `expect` block with `final_state` (`finished` | `paused`), `nodes`
//! and `tasks`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use serde::Deserialize;
use yunta_adapters::{Adapter, MockAdapter, MOCK_ID};
use yunta_core::{AdapterId, ModeName, RunId, SystemClock, Workflow};
use yunta_engine::{NodeState, RunEnv, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::AsyncStorage;

use super::status::task_status_label;
use crate::load_yaml;
use crate::project;

/// The run id every case's single run is created under.
static TEST_RUN: RunId = RunId::from_static("test-run");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestCase {
    /// Workflow name, resolved to `.yunta/workflows/<name>.yaml`.
    workflow: String,
    /// Mode the run is created in. Absent runs the whole graph, under
    /// the same `"default"` a workflow with no `modes:` runs under.
    #[serde(default)]
    mode: Option<ModeName>,
    /// Values for the workflow's declared inputs, by name. Every scalar
    /// arrives as text and `resolve_inputs` types it against the
    /// declaration, exactly as `yunta run --input` does.
    #[serde(default)]
    inputs: BTreeMap<String, String>,
    /// Directory whose contents seed the sandbox worktree, relative to
    /// the case file; absent, the sandbox is an empty repository.
    #[serde(default)]
    worktree: Option<PathBuf>,
    /// Fixture path, relative to the case file.
    fixture: PathBuf,
    expect: Expect,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    final_state: FinalState,
    #[serde(default)]
    nodes: BTreeMap<String, String>,
    #[serde(default)]
    tasks: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FinalState {
    Finished,
    Paused,
    /// Never actually reachable under `NoInteraction` — the `promote`
    /// option needs a live `HumanInteraction` to be chosen at all —
    /// kept for schema completeness rather than making `RunTerminal`'s
    /// mapping here partial.
    Promoted,
}

pub async fn test(dir: Option<&Path>) -> ExitCode {
    let root = match project_root(dir) {
        Ok(root) => root,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let tests_dir = root.join(".yunta/tests");
    let Some(case_paths) = discover_case_paths(&root) else {
        eprintln!("error: cannot read test cases from {}", tests_dir.display());
        return ExitCode::FAILURE;
    };
    if case_paths.is_empty() {
        eprintln!("error: no test cases found under {}", tests_dir.display());
        return ExitCode::FAILURE;
    }

    let mut failures = 0usize;
    for case_path in &case_paths {
        let name = case_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| case_path.display().to_string());
        match run_case(&root, case_path).await {
            Ok(problems) if problems.is_empty() => println!("case {name} ... ok"),
            Ok(problems) => {
                failures += 1;
                println!("case {name} ... FAILED");
                for problem in problems {
                    println!("  {problem}");
                }
            }
            Err(error) => {
                failures += 1;
                println!("case {name} ... ERROR");
                println!("  {error}");
            }
        }
    }

    println!("{} case(s), {} failed", case_paths.len(), failures);
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
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

/// Runs one case; `Ok` carries assertion failures (empty = pass), `Err`
/// carries setup/execution errors.
pub(crate) async fn run_case(cwd: &Path, case_path: &Path) -> Result<Vec<String>, String> {
    let case: TestCase = load_yaml(case_path, "test case")
        .map_err(|_| format!("could not load test case `{}`", case_path.display()))?;

    let workflow_path = cwd
        .join(".yunta/workflows")
        .join(format!("{}.yaml", case.workflow));
    let workflow: Workflow = load_yaml(&workflow_path, "workflow")
        .map_err(|_| format!("could not load workflow `{}`", workflow_path.display()))?;

    let config = project::resolve(cwd).map_err(|e| e.to_string())?.config;

    // Sandbox: worktree + runs root + event log, all temp.
    let sandbox = tempfile::tempdir().map_err(|e| format!("cannot create sandbox: {e}"))?;
    let worktree = sandbox.path().join("worktree");
    std::fs::create_dir_all(&worktree).map_err(|e| e.to_string())?;
    if let Some(seed) = &case.worktree {
        let seed = case_path.parent().unwrap_or(Path::new(".")).join(seed);
        copy_dir_all(&seed, &worktree)
            .map_err(|e| format!("cannot seed the sandbox from `{}`: {e}", seed.display()))?;
    }
    init_git(&worktree)?;
    let runs_root = sandbox.path().join("runs");
    let storage = AsyncStorage::open(sandbox.path().join("events.db"))
        .await
        .map_err(|e| e.to_string())?;

    let run_id = TEST_RUN.clone();
    let run_dir = runs_root.join(run_id.as_str());

    let fixture_path = case_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&case.fixture);
    let mock = load_mock_fixture(&fixture_path, &run_dir, &worktree)?;
    let adapters = mock_adapters(&config, mock);

    let provided_inputs: HashMap<String, String> = case.inputs.into_iter().collect();
    let manifest = yunta_engine::build_manifest(
        &workflow,
        &config,
        workflow_path.parent().unwrap_or(Path::new(".")),
        &worktree,
        &provided_inputs,
    )
    .map_err(|e| e.to_string())?;

    // The case's `mode` is frozen into the run the way `--mode` is;
    // the default mode runs the whole graph unfiltered.
    let mode = case.mode.clone().unwrap_or_default();
    yunta_engine::create_run(
        yunta_engine::CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &runs_root,
            mode: &mode,
            promoted_from: None,
        },
        &storage,
        &SystemClock,
    )
    .await
    .map_err(|e| e.to_string())?;
    // A test case's every session comes from a scripted fixture — a
    // gate here has no human to ask, same as it has no LLM to call.
    let report = yunta_engine::execute_run(RunEnv {
        run_id: &run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &worktree,
        adapters: &adapters,
        storage: &storage,
        clock: &SystemClock,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &yunta_engine::NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .map_err(|e| e.to_string())?;

    // Compare against expect — every mismatch reported, not just the first.
    let mut problems = Vec::new();
    let got_state = match &report.terminal {
        RunTerminal::Finished => FinalState::Finished,
        RunTerminal::Paused { .. } => FinalState::Paused,
        RunTerminal::Promoted { .. } => FinalState::Promoted,
    };
    if got_state != case.expect.final_state {
        problems.push(format!(
            "final_state: expected {:?}, got {:?}",
            case.expect.final_state, report.terminal
        ));
    }
    for (node_id, expected) in &case.expect.nodes {
        let got = match report.state.nodes.get(node_id.as_str()) {
            Some(NodeState::Finished { .. }) => "finished",
            Some(NodeState::Failed { .. }) => "failed",
            Some(NodeState::Running { .. }) => "running",
            Some(NodeState::Waiting { .. }) => "waiting",
            None => "never ran",
        };
        if got != expected {
            problems.push(format!("node {node_id}: expected {expected}, got {got}"));
        }
    }
    for (task_id, expected) in &case.expect.tasks {
        let got = report
            .state
            .tasks
            .get(task_id.as_str())
            .map(task_status_label)
            .unwrap_or("never registered");
        if got != expected {
            problems.push(format!("task {task_id}: expected {expected}, got {got}"));
        }
    }
    Ok(problems)
}

/// Reads a mock fixture, renders it with the run's own paths
/// (`{{run.dir}}`, `{{worktree}}`) and parses it — the one way a
/// scripted session comes to exist, for `yunta test` and for
/// `yunta run --adapter mock --fixture` alike.
pub(crate) fn load_mock_fixture(
    fixture_path: &Path,
    run_dir: &Path,
    worktree: &Path,
) -> Result<Arc<MockAdapter>, String> {
    let fixture_text = std::fs::read_to_string(fixture_path)
        .map_err(|e| format!("cannot read fixture `{}`: {e}", fixture_path.display()))?;
    let vars = BTreeMap::from([
        ("run.dir".to_string(), run_dir.display().to_string()),
        ("worktree".to_string(), worktree.display().to_string()),
    ]);
    let rendered = yunta_engine::render_template(&fixture_text, &vars)
        .map_err(|e| format!("fixture `{}`: {e}", fixture_path.display()))?;
    MockAdapter::from_yaml(&rendered)
        .map(Arc::new)
        .map_err(|e| format!("fixture `{}`: {e}", fixture_path.display()))
}

/// The mock standing in for every adapter the config names, so the
/// same workflow resolves the same runners with no LLM behind them.
pub(crate) fn mock_adapters(
    config: &yunta_core::ConfigLayer,
    mock: Arc<MockAdapter>,
) -> HashMap<AdapterId, Arc<dyn Adapter>> {
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
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
fn init_git(dir: &Path) -> Result<(), String> {
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "yunta-test@localhost"],
        vec!["config", "user.name", "yunta test"],
        vec!["add", "-A"],
        vec!["commit", "-q", "--allow-empty", "-m", "sandbox"],
    ] {
        let status = std::process::Command::new("git")
            .args(&args)
            .current_dir(dir)
            .status()
            .map_err(|e| format!("git {args:?}: {e}"))?;
        if !status.success() {
            return Err(format!("git {args:?} failed in `{}`", dir.display()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_case_declares_its_mode_and_reads_every_input_scalar_as_text() {
        let case: TestCase = yunta_core::yaml::parse(
            "workflow: build-feature\n\
             mode: quick\n\
             inputs: { idea: add dark mode, retries: 3, dry_run: true }\n\
             fixture: fixtures/quick.yaml\n\
             expect:\n  final_state: paused\n",
        )
        .unwrap();
        assert_eq!(
            case.mode.as_ref().map(|value| value.as_str()),
            Some("quick")
        );
        assert_eq!(case.inputs["idea"], "add dark mode");
        assert_eq!(case.inputs["retries"], "3");
        assert_eq!(case.inputs["dry_run"], "true");
    }

    #[test]
    fn a_case_names_the_directory_that_seeds_its_sandbox() {
        let case: TestCase = yunta_core::yaml::parse(
            "workflow: ledger-task\n\
             worktree: worktrees/greeting-crate\n\
             fixture: fixtures/ledger-task.yaml\n\
             expect:\n  final_state: finished\n",
        )
        .unwrap();
        assert_eq!(
            case.worktree.as_deref(),
            Some(Path::new("worktrees/greeting-crate"))
        );
    }

    #[test]
    fn a_case_without_mode_or_inputs_runs_the_whole_graph_on_input_defaults() {
        let case: TestCase = yunta_core::yaml::parse(
            "workflow: review\nfixture: fixtures/review.yaml\nexpect:\n  final_state: finished\n",
        )
        .unwrap();
        assert_eq!(case.mode, None);
        assert!(case.inputs.is_empty());
        assert_eq!(case.worktree, None);
    }
}
