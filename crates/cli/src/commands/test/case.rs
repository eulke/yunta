//! What a case is, and what running one does.
//!
//! A case takes the same path a run takes, through the same functions:
//! the workflow resolves through the project's own catalog,
//! `check_or_refuse` refuses here whatever it would refuse there, the
//! manifest is frozen the same way, and the run is driven by the one
//! `execute` call this binary makes. What differs is only what a case
//! *is* — every session scripted, and nobody to ask beyond the answers
//! the case itself declares.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use yunta_core::{ModeName, NodeId, OptionId, WorkflowName};
use yunta_engine::RunTerminal;

use super::{copy_dir_all, init_git, load_mock_fixture, mock_adapters};
use crate::commands::status::task_status_label;
use crate::commands::Adapters;
use crate::context::Context;
use crate::error::CliError;
use crate::load_yaml;
use crate::render::state::RunWord;
use crate::render::StateWord;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestCase {
    /// Workflow name, resolved to `.yunta/workflows/<name>.yaml`.
    workflow: WorkflowName,
    /// Mode the run is created in. Absent runs the whole graph, under
    /// the same `"default"` a workflow with no `modes:` runs under.
    #[serde(default)]
    mode: Option<ModeName>,
    /// Values for the workflow's declared inputs, by name. Every scalar
    /// arrives as text and `resolve_inputs` types it against the
    /// declaration, exactly as `yunta run --input` does.
    #[serde(default)]
    inputs: BTreeMap<yunta_core::InputName, String>,
    /// Directory whose contents seed the sandbox worktree, relative to
    /// the case file; absent, the sandbox is an empty repository.
    #[serde(default)]
    worktree: Option<PathBuf>,
    /// Fixture path, relative to the case file.
    fixture: PathBuf,
    /// What each gate is answered with, by node id — the option a
    /// person would pick off that gate's own menu.
    ///
    /// A case answers a gate the way anyone does: the run parks on it,
    /// the decision goes on the log, and the run is handed back. So a
    /// gate no entry here names is a gate nobody answers, and the run
    /// parks on it for good — which is what most cases assert. An entry
    /// naming an option the gate does not offer fails the case, because
    /// the case is then describing a menu the workflow does not have.
    #[serde(default)]
    decisions: BTreeMap<NodeId, OptionId>,
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

/// The terminal a run actually reached, with the reason a person needs
/// laid out under it. `Debug` would wrap the reason in quotes and escape
/// every one it contains — noise added to a message already written for
/// a reader.
fn terminal_label(terminal: &RunTerminal) -> String {
    let word = RunWord::of_terminal(terminal);
    match terminal {
        RunTerminal::Paused { reason } | RunTerminal::Failed { reason } => {
            format!("{word}\n    {}", yunta_core::text::hanging(reason, "    "))
        }
        RunTerminal::Finished | RunTerminal::Promoted { .. } => word.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FinalState {
    Finished,
    Paused,
    /// The run closed as failed — a node failed under
    /// `defaults.on_failure: abort | continue`.
    Failed,
    /// The run closed by promoting to a later mode — reached by a case
    /// whose `decisions:` answers the gate that offers `promote`.
    Promoted,
}

impl FinalState {
    /// The word a case file's `final_state` is written in, which is the
    /// word every surface calls that stop by: a case is asserted in the
    /// vocabulary the run is reported in, so neither can drift from the
    /// other.
    fn word(self) -> RunWord {
        match self {
            FinalState::Finished => RunWord::Finished,
            FinalState::Paused => RunWord::Paused,
            FinalState::Failed => RunWord::Failed,
            FinalState::Promoted => RunWord::Promoted,
        }
    }
}

/// Runs one case; `Ok` carries assertion failures (empty = pass), `Err`
/// carries setup/execution errors.
///
/// The failures are the case's own verdict and read as sentences a
/// person compares; the error is the CLI's one error type, so a case
/// that could not run at all says why in exactly the words the command
/// it stands in for would have used.
pub(crate) async fn run_case(cwd: &Path, case_path: &Path) -> Result<Vec<String>, CliError> {
    let case: TestCase = load_yaml(case_path, "test case").map_err(|_| {
        CliError::msg(format!(
            "could not load test case `{}`",
            case_path.display()
        ))
    })?;
    let beside = |path: &Path| case_path.parent().unwrap_or(Path::new(".")).join(path);

    // The sandbox is a checkout of its own: this project's `.yunta/`
    // copied in, whatever the case seeds on top, and a git repository
    // around both. A case then resolves its workflow through the very
    // catalog a run resolves through, and `runnable` refuses here
    // exactly what it refuses there.
    let sandbox = tempfile::tempdir().map_err(|e| CliError::io("create", "a sandbox", e))?;
    let worktree = sandbox.path().join("worktree");
    std::fs::create_dir_all(&worktree)
        .map_err(|e| CliError::io("create", worktree.display(), e))?;
    let catalog = cwd.join(".yunta");
    if catalog.is_dir() {
        copy_dir_all(&catalog, &worktree.join(".yunta"))
            .map_err(|e| CliError::io("copy the catalog from", catalog.display(), e))?;
    }
    if let Some(seed) = &case.worktree {
        let seed = beside(seed);
        copy_dir_all(&seed, &worktree)
            .map_err(|e| CliError::io("seed the sandbox from", seed.display(), e))?;
    }
    init_git(&worktree)?;

    let ctx = Context::resolve_in(cwd.to_path_buf())?.sandboxed(worktree, sandbox.path());
    let storage = ctx.async_storage().await?;
    let fixture_path = beside(&case.fixture);

    // The same three steps `yunta run` takes, in the same order and
    // through the same functions: resolve and check the workflow,
    // freeze its manifest, create the run, drive it. What differs is
    // only what a case *is* — every session scripted, nobody to ask.
    let raw_inputs: Vec<String> = case
        .inputs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    let (frozen, _) = crate::commands::run::runnable(
        &ctx,
        Path::new(case.workflow.as_str()),
        &raw_inputs,
        None,
        Some(&fixture_path),
    )
    .await?;
    let manifest = frozen.manifest.clone();
    let prepared =
        crate::commands::run::create_run_from(&ctx, &storage, &frozen, case.mode.as_ref()).await?;

    let mock = load_mock_fixture(&fixture_path, &prepared.run_dir, &prepared.worktree)?;
    let adapters = mock_adapters(&ctx.project.config, Arc::clone(&mock));
    let report = crate::commands::drive::execute(crate::commands::drive::Executing {
        run_id: &prepared.run_id,
        manifest: &manifest,
        run_dir: &prepared.run_dir,
        worktree: &prepared.worktree,
        adapters: &adapters,
        storage: &storage,
        clock: Arc::new(ctx.clock),
        ids: &ctx.ids,
        // A test case's every session comes from a scripted fixture — a
        // gate here has no human to ask, same as it has no LLM to call.
        human_interaction: &yunta_engine::NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        // A case's verdict is its report, compared against `expect:` —
        // there is no live surface drawing it.
        observer: None,
        fence_hook: ctx.fence_hook.clone(),
        // A test case is hermetic: no ambient user knowledge from the
        // developer's own `~/.yunta`, and its nodes inherit this
        // process's environment with nothing injected.
        ambient: &yunta_core::Env::default(),
    })
    .await?;
    let report = answered(
        &ctx,
        &storage,
        &prepared,
        &manifest,
        &adapters,
        case.decisions,
        report,
    )
    .await?;

    // Compare against expect — every mismatch reported, not just the first.
    let mut problems = Vec::new();

    // A fixture that describes sessions which never happened describes
    // a run nobody made: the case fails, always, and names which
    // scripts went unclaimed. There is no expectation to opt into —
    // the fixture is the case's own account of what the run does.
    let unclaimed = mock.unconsumed();
    if !unclaimed.is_empty() {
        problems.push(format!(
            "fixture `{}`: {} nothing opened — {}",
            fixture_path.display(),
            yunta_core::text::counted(unclaimed.len(), "scripted session"),
            unclaimed
                .iter()
                .map(|index| format!("#{}", index + 1))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let expected_word = case.expect.final_state.word();
    if RunWord::of_terminal(&report.terminal) != expected_word {
        problems.push(format!(
            "final_state: expected {expected_word}, got {}",
            terminal_label(&report.terminal)
        ));
    }
    for (node_id, expected) in &case.expect.nodes {
        let got = StateWord::of(report.state.nodes.state(node_id.as_str())).word();
        if got != expected {
            problems.push(format!("node {node_id}: expected {expected}, got {got}"));
        }
    }
    for (task_id, expected) in &case.expect.tasks {
        let got = report
            .state
            .tasks
            .status(task_id.as_str())
            .map(task_status_label)
            .unwrap_or("never registered");
        if got != expected {
            problems.push(format!("task {task_id}: expected {expected}, got {got}"));
        }
    }
    Ok(problems)
}

/// Answers each gate the case names and hands the run back, until the
/// run stops somewhere no answer applies.
///
/// A case answers a gate exactly as a person does and through exactly
/// the same door: the run parks, `resolve_gate` puts the decision on
/// the log, and the run is driven again — the engine consumes the
/// pre-seeded decision through its one consequence path. Each answer is
/// spent once, so a workflow that parks on the same gate twice stops
/// there the second time rather than looping.
#[allow(clippy::too_many_arguments)]
async fn answered(
    ctx: &Context,
    storage: &yunta_storage::AsyncStorage,
    prepared: &crate::commands::drive::Prepared,
    manifest: &yunta_core::Manifest,
    adapters: &Adapters,
    mut decisions: BTreeMap<NodeId, OptionId>,
    mut report: yunta_engine::RunReport,
) -> Result<yunta_engine::RunReport, CliError> {
    while !decisions.is_empty() {
        let events = storage.events_for_run(prepared.run_id.clone()).await?;
        let state = yunta_engine::derive(&events);
        let Some((node, _)) = yunta_engine::current_escalation(manifest, &state) else {
            break;
        };
        let Some(option) = decisions.remove(&node) else {
            break;
        };
        yunta_engine::resolve_gate(
            manifest,
            storage,
            &prepared.run_id,
            &ctx.clock,
            yunta_core::events::HumanChoice {
                option,
                by: crate::identity::responder(None),
                free_text: None,
            },
        )
        .await
        .map_err(|refusal| CliError::gate_refused(&prepared.run_id, refusal))?;
        report = crate::commands::drive::execute(crate::commands::drive::Executing {
            run_id: &prepared.run_id,
            manifest,
            run_dir: &prepared.run_dir,
            worktree: &prepared.worktree,
            adapters,
            storage,
            clock: Arc::new(ctx.clock),
            ids: &ctx.ids,
            human_interaction: &yunta_engine::NoInteraction,
            forge: None,
            cancel: None,
            adapter_override: None,
            observer: None,
            fence_hook: ctx.fence_hook.clone(),
            ambient: &yunta_core::Env::default(),
        })
        .await?;
    }
    Ok(report)
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
            "workflow: run-tasks\n\
             worktree: worktrees/greeting-crate\n\
             fixture: fixtures/run-tasks.yaml\n\
             expect:\n  final_state: finished\n",
        )
        .unwrap();
        assert_eq!(
            case.worktree.as_deref(),
            Some(Path::new("worktrees/greeting-crate"))
        );
    }

    #[test]
    fn a_case_declares_the_option_each_gate_is_answered_with() {
        let case: TestCase = yunta_core::yaml::parse(
            "workflow: build-feature\n\
             fixture: fixtures/quick.yaml\n\
             decisions: { approve-plan: approve, ship: promote }\n\
             expect:\n  final_state: promoted\n",
        )
        .unwrap();
        assert_eq!(case.decisions["approve-plan"].as_str(), "approve");
        assert_eq!(case.decisions["ship"].as_str(), "promote");
        assert_eq!(case.expect.final_state, FinalState::Promoted);
    }

    #[test]
    fn a_case_that_answers_no_gate_parks_on_the_first_one() {
        let case: TestCase = yunta_core::yaml::parse(
            "workflow: review\nfixture: fixtures/review.yaml\nexpect:\n  final_state: paused\n",
        )
        .unwrap();
        assert!(case.decisions.is_empty());
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
