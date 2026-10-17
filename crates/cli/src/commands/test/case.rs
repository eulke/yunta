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
use yunta_core::events::PauseReason;
use yunta_core::{ModeName, NodeId, OptionId, WorkflowName};
use yunta_engine::RunTerminal;

use super::{copy_dir_all, init_git, sandboxed_checkout};
use crate::commands::advice;
use crate::commands::registry::{adapters_and_script, execute, Executing, ScriptedPaths};
use crate::commands::Adapters;
use crate::context::Context;
use crate::error::CliError;
use crate::load_yaml;
use crate::render::state::RunWord;

mod problems;

use problems::{standing_problems, unclaimed_problems};

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
pub(super) fn terminal_label(word: RunWord, terminal: &RunTerminal) -> String {
    match terminal {
        RunTerminal::Paused { reason } => format!(
            "{word}\n    {}",
            yunta_core::text::hanging(&yunta_render::pause::sentence(reason), "    ")
        ),
        RunTerminal::Failed { node, failure } => {
            let said = PauseReason::NodeFailed {
                node: node.clone(),
                failure: failure.clone(),
            };
            format!(
                "{word}\n    {}",
                yunta_core::text::hanging(&yunta_render::pause::sentence(&said), "    ")
            )
        }
        RunTerminal::Finished | RunTerminal::Promoted { .. } => word.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
enum FinalState {
    Finished,
    /// The run stopped on a person: a decision, a question, a budget.
    NeedsYou,
    /// The run is parked until a limit resets, and resumes on its own then.
    Waiting,
    /// The run finished holding findings that block it.
    Reported,
    /// The run closed as failed — a node failed under
    /// `defaults.on_failure: abort | continue`.
    Failed,
    /// The run closed by promoting to a later mode — reached by a case
    /// whose `decisions:` answers the gate that offers `promote`.
    Promoted,
}

impl TryFrom<String> for FinalState {
    type Error = String;

    /// The word a case writes, in the vocabulary every surface reports a
    /// run in. `paused` is a word that vocabulary retired, and the case
    /// is told the one that replaced it.
    fn try_from(word: String) -> Result<Self, String> {
        match word.as_str() {
            "finished" => Ok(FinalState::Finished),
            "needs you" => Ok(FinalState::NeedsYou),
            "waiting" => Ok(FinalState::Waiting),
            "reported" => Ok(FinalState::Reported),
            "failed" => Ok(FinalState::Failed),
            "promoted" => Ok(FinalState::Promoted),
            "paused" => Err(
                "`paused` is called `needs you` now: a run stopped on a person needs you"
                    .to_string(),
            ),
            other => Err(format!(
                "`{other}` is not a final state; one of: finished, needs you, waiting, \
                 reported, failed, promoted"
            )),
        }
    }
}

impl FinalState {
    /// The word a case file's `final_state` is written in, which is the
    /// word every surface calls that stop by: a case is asserted in the
    /// vocabulary the run is reported in, so neither can drift from the
    /// other.
    fn word(self) -> RunWord {
        match self {
            FinalState::Finished => RunWord::Finished,
            FinalState::NeedsYou => RunWord::NeedsYou,
            FinalState::Waiting => RunWord::Waiting,
            FinalState::Reported => RunWord::Reported,
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
pub(crate) async fn run_case(
    cwd: &Path,
    case_path: &Path,
    interrupt: crate::interrupt::Interrupt,
) -> Result<Vec<String>, CliError> {
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
    let sandbox = sandboxed_checkout(cwd)?;
    if let Some(seed) = &case.worktree {
        let seed = beside(seed);
        copy_dir_all(&seed, sandbox.worktree())
            .map_err(|e| CliError::io("seed the sandbox from", seed.display(), e))?;
    }
    let ctx = Context::resolve_in(cwd.to_path_buf(), interrupt)?;
    init_git(sandbox.worktree(), ctx.supervision()).await?;
    let ctx = ctx.sandboxed(sandbox.worktree().to_path_buf(), sandbox.root());
    let storage = ctx.async_storage().await?;
    let fixture_path = beside(&case.fixture);
    let played = play(&ctx, &storage, &case, &fixture_path).await?;
    Ok(verdict(&case, &fixture_path, &played))
}

/// What a played case leaves for its verdict: the run's manifest and
/// preparation, its report, and the scripts its sessions may not all claim.
struct Played {
    manifest: yunta_core::Manifest,
    prepared: crate::commands::drive::Prepared,
    report: yunta_engine::RunReport,
    script: Option<Arc<yunta_adapters::MockAdapter>>,
}

/// Freezes the case's workflow and creates its run, through the steps
/// `yunta run` takes in the same order. What differs is only what a case
/// *is* — every session scripted, nobody to ask.
async fn create_case_run(
    ctx: &Context,
    storage: &yunta_storage::AsyncStorage,
    case: &TestCase,
    fixture_path: &Path,
) -> Result<(yunta_core::Manifest, crate::commands::drive::Prepared), CliError> {
    let raw_inputs: Vec<String> = case
        .inputs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    // A case is hermetic at birth too: its manifest freezes no user
    // knowledge, as its sessions read none.
    let frozen = crate::commands::run::runnable(
        ctx,
        Path::new(case.workflow.as_str()),
        &raw_inputs,
        None,
        Some(fixture_path),
        case.mode.as_ref(),
        &yunta_core::Env::default(),
    )
    .await?;
    let prepared =
        crate::commands::run::create_run_from(ctx, storage, &frozen, case.mode.as_ref()).await?;
    Ok((frozen.manifest, prepared))
}

/// Drives the case's run under its scripted sessions, answering each gate
/// the case names. Also answers with the scripted mock the run's sessions
/// claimed from, so the verdict can name the scripts nothing claimed.
async fn drive_case_run(
    ctx: &Context,
    storage: &yunta_storage::AsyncStorage,
    case: &TestCase,
    manifest: &yunta_core::Manifest,
    prepared: &crate::commands::drive::Prepared,
) -> Result<
    (
        yunta_engine::RunReport,
        Option<Arc<yunta_adapters::MockAdapter>>,
    ),
    CliError,
> {
    let (adapters, script) = adapters_and_script(
        manifest,
        &ScriptedPaths {
            run_dir: &prepared.run_dir,
            worktree: &prepared.worktree,
        },
    )?;
    let forge = stand_in_forge(&manifest.config);
    let forge = forge
        .as_ref()
        .map(|forge| forge as &dyn yunta_core::port::Forge);
    let report = execute(Executing {
        run_id: &prepared.run_id,
        manifest,
        run_dir: &prepared.run_dir,
        worktree: &prepared.worktree,
        adapters: &adapters,
        storage,
        clock: Arc::new(ctx.clock),
        ids: &ctx.ids,
        // A test case's every session comes from a scripted fixture — a
        // gate here has no human to ask, same as it has no LLM to call.
        human_interaction: &yunta_engine::NoInteraction,
        forge,
        cancel: ctx.cancellation(),
        // A case's verdict is its report, compared against `expect:` —
        // there is no live surface drawing it.
        observer: None,
        rail_command: ctx.rail_command.clone(),
        // A test case is hermetic: no ambient user knowledge from the
        // developer's own `~/.yunta`, and its nodes inherit this
        // process's environment with nothing injected.
        ambient: &yunta_core::Env::default(),
    })
    .await?;
    let report = answered(
        ctx,
        storage,
        prepared,
        manifest,
        &adapters,
        forge,
        case.decisions.clone(),
        report,
    )
    .await?;
    Ok((report, script))
}

/// Plays the case: creates its run, then drives it.
async fn play(
    ctx: &Context,
    storage: &yunta_storage::AsyncStorage,
    case: &TestCase,
    fixture_path: &Path,
) -> Result<Played, CliError> {
    let (manifest, prepared) = create_case_run(ctx, storage, case, fixture_path).await?;
    let (report, script) = drive_case_run(ctx, storage, case, &manifest, &prepared).await?;
    Ok(Played {
        manifest,
        prepared,
        report,
        script,
    })
}

/// Every mismatch between what the case played and what it expects, each
/// reported as a sentence a person compares.
fn verdict(case: &TestCase, fixture_path: &Path, played: &Played) -> Vec<String> {
    let mut problems = unclaimed_problems(fixture_path, played.script.as_ref());
    problems.extend(standing_problems(case, played));
    for (task_id, expected) in &case.expect.tasks {
        let got = played
            .report
            .state
            .tasks
            .status(task_id.as_str())
            .map(yunta_core::events::TaskStatus::as_str)
            .unwrap_or("never registered");
        if got != expected {
            problems.push(format!("task {task_id}: expected {expected}, got {got}"));
        }
    }
    problems
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
    forge: Option<&dyn yunta_core::port::Forge>,
    mut decisions: BTreeMap<NodeId, OptionId>,
    mut report: yunta_engine::RunReport,
) -> Result<yunta_engine::RunReport, CliError> {
    while !decisions.is_empty() {
        let Some(node) = yunta_engine::awaits_decision(manifest, &report.state) else {
            break;
        };
        let Some(option) = decisions.remove(&node) else {
            break;
        };
        let redactor = yunta_engine::run_redactor(manifest, &yunta_core::ProcessSecrets)?;
        yunta_engine::resolve_gate(
            manifest,
            &prepared.run_dir,
            storage,
            &prepared.run_id,
            &ctx.clock,
            yunta_core::events::HumanChoice {
                option,
                by: crate::identity::responder(None, ctx.env.user.as_deref()),
                free_text: None,
            },
            &redactor,
        )
        .await
        .map_err(|refusal| CliError::gate_refused(&prepared.run_id, refusal))?;
        report = execute(Executing {
            run_id: &prepared.run_id,
            manifest,
            run_dir: &prepared.run_dir,
            worktree: &prepared.worktree,
            adapters,
            storage,
            clock: Arc::new(ctx.clock),
            ids: &ctx.ids,
            human_interaction: &yunta_engine::NoInteraction,
            forge,
            cancel: ctx.cancellation(),
            observer: None,
            rail_command: ctx.rail_command.clone(),
            ambient: &yunta_core::Env::default(),
        })
        .await?;
    }
    Ok(report)
}

/// The forge a case's run reaches: a mock standing in for the one the
/// config declares, the way the mock adapter stands in for every
/// runner — a case never reaches a real forge. `None` when the config
/// declares none.
fn stand_in_forge(config: &yunta_core::ConfigLayer) -> Option<yunta_adapters::MockForge> {
    config
        .forge
        .as_ref()
        .and_then(|forge| forge.github.as_ref())
        .map(|_| {
            yunta_adapters::MockForge::new(
                yunta_adapters::MockForgeState::new(),
                Arc::new(advice::spell),
            )
        })
}

#[cfg(test)]
mod tests;
