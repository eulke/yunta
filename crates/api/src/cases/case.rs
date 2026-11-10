//! See [`super`]. What a case is, and what running one does.
//!
//! A case takes the same path a run takes, through the same functions: the
//! workflow resolves through the project's own catalog, `check` refuses here
//! whatever it would refuse there, the manifest is frozen the same way, and
//! the run is driven by the one `execute` a drive makes. What differs is only
//! what a case *is*: every session scripted, nobody to ask beyond the answers
//! the case itself declares, no forge but a stand-in, and no environment of
//! the person's own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use surco::words::Standing;
use yunta_adapters::MockAdapter;
use yunta_core::events::TaskStatus;
use yunta_core::events::{DecisionOn, HumanChoice, PauseReason};
use yunta_core::port::{Forge, NoInteraction};
use yunta_core::WorkflowName;
use yunta_core::{Env, Manifest, ModeName, NodeId, OptionId, Responder, RunAdapter, RunId};
use yunta_engine::lock::Contention;
use yunta_engine::{RunReport, RunTerminal};
use yunta_render::page::CaseProblem;

use crate::drive::{adapters_and_script, Execution, Means};
use crate::sandbox::Sandbox;
use crate::{Error, Failure, Ports, Project, Remarks, RunRequest, Stop, Workspace};

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

/// What stopped a run that ended on `terminal`: the pause it parked on, or
/// the node that failed it, said as a pause; nothing for a run that finished
/// or was promoted.
pub(super) fn stopped_by(terminal: &RunTerminal) -> Option<PauseReason> {
    match terminal {
        RunTerminal::Paused { reason } => Some(reason.clone()),
        RunTerminal::Failed { node, failure } => Some(PauseReason::NodeFailed {
            node: node.clone(),
            failure: failure.clone(),
        }),
        RunTerminal::Finished | RunTerminal::Promoted { .. } => None,
    }
}

/// The word a case's run ends on, in the vocabulary every surface reports
/// a run in: a case is asserted in the words the run is reported in, so
/// neither can drift from the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
struct FinalState(Standing);

impl FinalState {
    /// The words a run a case plays can end on.
    const ENDS: [Standing; 7] = [
        Standing::Finished,
        Standing::NeedsYou,
        Standing::Waiting,
        Standing::Stopped,
        Standing::Reported,
        Standing::Failed,
        Standing::Promoted,
    ];

    fn word(self) -> Standing {
        self.0
    }
}

impl TryFrom<String> for FinalState {
    type Error = String;

    /// The word a case writes. A word the vocabulary retired is told the
    /// one that replaced it.
    fn try_from(word: String) -> Result<Self, String> {
        match Standing::named(&word) {
            Some(standing) if Self::ENDS.contains(&standing) => Ok(FinalState(standing)),
            _ => Err(yunta_render::page::not_a_final_state(&word, &Self::ENDS)),
        }
    }
}

/// Runs one case found under `root`; `Ok` carries assertion failures
/// (empty = pass), `Err` what stopped it from running at all, said in exactly
/// the words the run it stands in for would have used.
pub(super) async fn run_case(
    workspace: &Workspace,
    root: &Path,
    case_path: &Path,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<Vec<CaseProblem>, Error> {
    let case = read_case(case_path)?;
    let beside = |path: &Path| case_path.parent().unwrap_or(Path::new(".")).join(path);
    // The sandbox is a checkout of its own: the project's `.yunta/` copied
    // in, whatever the case seeds on top, and a git repository around both.
    // A case then resolves its workflow through the very catalog a run
    // resolves through, and `check` refuses here exactly what it refuses
    // there.
    let sandbox = Sandbox::seeded(root)?;
    if let Some(seed) = &case.worktree {
        sandbox.seed(&beside(seed))?;
    }
    let project = workspace.project_at(root)?;
    sandbox
        .commit_seed(project.workspace.supervision(stop))
        .await?;
    let project = project.sandboxed(&sandbox);
    let fixture_path = beside(&case.fixture);
    let played = play(&project, &case, &fixture_path, remarks, stop).await?;
    Ok(verdict(&case, &fixture_path, &played))
}

/// The case at `path`, read: refused, naming the file and why, when it does
/// not read as one.
fn read_case(path: &Path) -> Result<TestCase, Error> {
    let unread = |why| {
        Failure::new(CaseUnread {
            path: path.to_path_buf(),
            why,
        })
    };
    let text = std::fs::read_to_string(path).map_err(|source| unread(Unreadable::Disk(source)))?;
    yunta_core::yaml::parse(&text).map_err(|source| unread(Unreadable::Case(source)).into())
}

/// A case file that does not read as a case: where it is, and why, with every
/// cause behind it, so a retired word is told the one that replaced it.
#[derive(Debug, thiserror::Error)]
#[error("could not load test case `{}`: {}", path.display(), yunta_core::describe(why))]
struct CaseUnread {
    path: PathBuf,
    why: Unreadable,
}

/// Why a case file does not read: the disk refused it, or its text is no case.
#[derive(Debug, thiserror::Error)]
enum Unreadable {
    #[error(transparent)]
    Disk(std::io::Error),
    #[error(transparent)]
    Case(yunta_core::yaml::YamlError),
}

/// What a played case leaves for its verdict: the run it made and froze,
/// its report, and the scripts its sessions may not all claim.
struct Played {
    manifest: Manifest,
    run_id: RunId,
    report: RunReport,
    script: Option<Arc<MockAdapter>>,
}

/// Plays the case: admits and creates its run through the steps a run
/// takes, in the same order, then drives it under its scripted sessions,
/// answering each gate the case names.
async fn play(
    project: &Project,
    case: &TestCase,
    fixture_path: &Path,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<Played, Error> {
    let fixture = crate::machine::read_fixture(fixture_path).await?;
    let request = RunRequest {
        workflow: Path::new(case.workflow.as_str()),
        inputs: case.inputs.clone().into_iter().collect(),
        mode: case.mode.as_ref(),
        adapter: Some(RunAdapter::Mock { fixture }),
        // A case is hermetic at birth too: its manifest freezes no user
        // knowledge, as its sessions read none.
        hermetic: true,
    };
    let admission = project.admit(request, remarks, stop).await?;
    let created = admission.create(remarks, stop).await?;
    let (adapters, script) =
        adapters_and_script(&created.manifest, &created.dir, &created.worktree)?;
    let means = Means {
        adapters,
        forge: stand_in_forge(&created.manifest.config),
        storage: project.storage().await?,
        // A case's every session comes from a scripted fixture: a gate here
        // has no person to ask, as it has no model to call, and its verdict
        // is its report, with no surface drawing it.
        ports: Ports {
            person: &NoInteraction,
            observer: None,
        },
        // A case is hermetic: no ambient knowledge of the person's own, and
        // its nodes inherit this process's environment with nothing added.
        ambient: &Env::default(),
    };
    let on = CaseRun {
        project,
        id: &created.id,
        dir: &created.dir,
        manifest: &created.manifest,
        worktree: &created.worktree,
        means: &means,
    };
    let report = on.execute(stop).await?;
    let report = on.answered(case.decisions.clone(), report, stop).await?;
    Ok(Played {
        manifest: created.manifest.clone(),
        run_id: created.id.clone(),
        report,
        script,
    })
}

/// A case's run, created, as it is driven and answered.
struct CaseRun<'a> {
    project: &'a Project,
    id: &'a RunId,
    dir: &'a Path,
    manifest: &'a Manifest,
    worktree: &'a Path,
    means: &'a Means<'a>,
}

impl CaseRun<'_> {
    /// Claims the run, anyone else holding it refused at once, and drives it
    /// to its stop.
    async fn execute(&self, stop: &Stop) -> Result<RunReport, Error> {
        let clock = self.project.workspace.clock.as_ref();
        let hold = yunta_engine::hold_run(self.dir, Contention::Refuse, clock, stop.work())
            .await
            .map_err(yunta_engine::RunError::from)?;
        let execution = Execution {
            id: self.id,
            manifest: self.manifest,
            hold,
            worktree: self.worktree,
        };
        self.project.execute(execution, self.means, stop).await
    }

    /// Answers each gate the case names and drives the run again, until
    /// the run stops somewhere no answer applies.
    ///
    /// A case answers a gate exactly as a person does and through exactly
    /// the same door: the run parks, `resolve_gate` puts the decision on the
    /// log, and the run is driven again, the engine consuming the decision
    /// through its one consequence path. Each answer is spent once, so a
    /// workflow that parks on the same gate twice stops there the second time
    /// rather than looping.
    async fn answered(
        &self,
        mut decisions: BTreeMap<NodeId, OptionId>,
        mut report: RunReport,
        stop: &Stop,
    ) -> Result<RunReport, Error> {
        let workspace = &self.project.workspace;
        while !decisions.is_empty() {
            // A case names its answers by node, so a decision the run owns
            // ends its answers.
            let Some(DecisionOn::Node(node)) =
                yunta_engine::awaits_decision(self.manifest, &report.state)
            else {
                break;
            };
            let Some(option) = decisions.remove(&node) else {
                break;
            };
            let redactor = yunta_engine::run_redactor(self.manifest, workspace.secrets.as_ref())?;
            let choice = HumanChoice {
                option,
                by: Responder::answering(None, workspace.env.user.as_deref()),
                free_text: None,
            };
            let storage = &self.means.storage;
            let clock = workspace.clock.as_ref();
            yunta_engine::resolve_gate(
                self.manifest,
                self.dir,
                storage,
                self.id,
                clock,
                choice,
                &redactor,
            )
            .await
            .map_err(|error| crate::answers::unrecorded(self.id, error))?;
            report = self.execute(stop).await?;
        }
        Ok(report)
    }
}

/// Every mismatch between what the case played and what it expects, each
/// one render says as a line a person compares.
fn verdict(case: &TestCase, fixture_path: &Path, played: &Played) -> Vec<CaseProblem> {
    let mut problems = unclaimed_problems(fixture_path, played.script.as_ref());
    problems.extend(standing_problems(case, played));
    for (task, expected) in &case.expect.tasks {
        let got = played.report.state.tasks.status(task.as_str());
        if got.map(TaskStatus::as_str) != Some(expected.as_str()) {
            problems.push(CaseProblem::Task {
                task: task.clone(),
                expected: expected.clone(),
                got,
            });
        }
    }
    problems
}

/// The forge a case's run reaches: a mock standing in for the one the
/// config declares, the way the mock adapter stands in for every runner, so
/// a case never reaches a real forge. `None` when the config declares none.
fn stand_in_forge(config: &yunta_core::ConfigLayer) -> Option<Arc<dyn Forge>> {
    config
        .forge
        .as_ref()
        .and_then(|forge| forge.github.as_ref())
        .map(|_| {
            let state = yunta_adapters::MockForgeState::new();
            Arc::new(yunta_adapters::MockForge::new(state)) as Arc<dyn Forge>
        })
}

#[cfg(test)]
mod tests;
