//! `yunta test` (T7.9 recorte, Contrato §14): discover cases under
//! `.yunta/tests/`, execute each workflow with the `mock` adapter driven
//! by the case's fixture, derive the final state by replay and compare
//! it against `expect`. No LLM, no network, deterministic.
//!
//! Each case runs in its own sandbox: a fresh git worktree, a fresh runs
//! root and a fresh event-log DB under a temp dir — a test run never
//! touches the project's real state. The fixture file is rendered with
//! `{{run.dir}}` and `{{worktree}}` before parsing, so a scripted
//! session can place artifacts exactly where a real agent (told
//! `{{run.dir}}` in its prompt) would.
//!
//! M-0 cut of the §14 case format: `workflow`, `fixture` and `expect`
//! with `final_state` (`finished` | `paused`), `nodes` and `tasks`.
//! `mode`/`inputs` wait for their schema; `events`/`never` clauses wait
//! for full T7.9.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use serde::Deserialize;
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::{RunId, SystemClock, Workflow};
use yunta_engine::{NodeState, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::Storage;

use super::status::task_status_label;
use crate::load_yaml;
use crate::project;

#[derive(Debug, Deserialize)]
struct TestCase {
    /// Workflow name, resolved to `.yunta/workflows/<name>.yaml`.
    workflow: String,
    /// Fixture path, relative to the case file.
    fixture: PathBuf,
    expect: Expect,
}

#[derive(Debug, Deserialize)]
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
}

pub async fn test() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    let tests_dir = cwd.join(".yunta/tests");
    let mut case_paths: Vec<PathBuf> = match std::fs::read_dir(&tests_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext == "yaml" || ext == "yml")
            })
            .collect(),
        Err(e) => {
            eprintln!(
                "error: cannot read test cases from {}: {e}",
                tests_dir.display()
            );
            return ExitCode::FAILURE;
        }
    };
    case_paths.sort();
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
        match run_case(&cwd, case_path).await {
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

/// Runs one case; `Ok` carries assertion failures (empty = pass), `Err`
/// carries setup/execution errors.
async fn run_case(cwd: &Path, case_path: &Path) -> Result<Vec<String>, String> {
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
    init_git(&worktree)?;
    let runs_root = sandbox.path().join("runs");
    let storage = Storage::open(&sandbox.path().join("events.db")).map_err(|e| e.to_string())?;

    let run_id = RunId::from("test-run");
    let run_dir = runs_root.join(run_id.as_str());

    // Render the fixture with the sandbox paths, then parse it.
    let fixture_path = case_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&case.fixture);
    let fixture_text = std::fs::read_to_string(&fixture_path)
        .map_err(|e| format!("cannot read fixture `{}`: {e}", fixture_path.display()))?;
    let vars = BTreeMap::from([
        ("run.dir".to_string(), run_dir.display().to_string()),
        ("worktree".to_string(), worktree.display().to_string()),
    ]);
    let rendered = yunta_engine::render_template(&fixture_text, &vars)
        .map_err(|e| format!("fixture `{}`: {e}", fixture_path.display()))?;
    let mock = Arc::new(
        MockAdapter::from_yaml(&rendered)
            .map_err(|e| format!("fixture `{}`: {e}", fixture_path.display()))?,
    );

    // The mock stands in for every adapter the config names — that is
    // the point of `yunta test`: same workflow, same runners, no LLM.
    let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), mock.clone());
    if let Some(runners) = &config.runners {
        for candidate in runners.values().flatten() {
            adapters
                .entry(candidate.adapter.clone())
                .or_insert_with(|| mock.clone());
        }
    }

    // Test cases don't declare input values yet (§14/T7.9's own recorte,
    // `docs/m0-status.md`) — every input a tested workflow declares must
    // have a `default`, same as any other consumer of `build_manifest`
    // that has none to offer.
    let manifest = yunta_engine::build_manifest(
        &workflow,
        &config,
        workflow_path.parent().unwrap_or(Path::new(".")),
        &worktree,
        &HashMap::new(),
    )
    .map_err(|e| e.to_string())?;

    yunta_engine::create_run(&run_id, &manifest, &runs_root, &storage, &SystemClock)
        .map_err(|e| e.to_string())?;
    let report = yunta_engine::execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &worktree,
        &adapters,
        &storage,
        &SystemClock,
        DEFAULT_MAX_RETRIES,
    )
    .await
    .map_err(|e| e.to_string())?;

    // Compare against expect — every mismatch reported, not just the first.
    let mut problems = Vec::new();
    let got_state = match &report.terminal {
        RunTerminal::Finished => FinalState::Finished,
        RunTerminal::Paused { .. } => FinalState::Paused,
    };
    if got_state != case.expect.final_state {
        problems.push(format!(
            "final_state: expected {:?}, got {:?}",
            case.expect.final_state, report.terminal
        ));
    }
    for (node_id, expected) in &case.expect.nodes {
        let got = match report.state.nodes.get(&node_id.as_str().into()) {
            Some(NodeState::Finished { .. }) => "finished",
            Some(NodeState::Failed { .. }) => "failed",
            Some(NodeState::Running { .. }) => "running",
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
            .get(&task_id.as_str().into())
            .map(task_status_label)
            .unwrap_or("never registered");
        if got != expected {
            problems.push(format!("task {task_id}: expected {expected}, got {got}"));
        }
    }
    Ok(problems)
}

fn init_git(dir: &Path) -> Result<(), String> {
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "yunta-test@localhost"],
        vec!["config", "user.name", "yunta test"],
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
