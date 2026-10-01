//! Proving a tasks document where its commands will run, the moment it
//! is handed over.
//!
//! A planner writes criteria in its own shell, whose `PATH` and dialect
//! are its CLI's, not the engine's. The engine runs criteria under `sh`
//! with the run's own environment, so a command that works where it was
//! written can fail where it counts — and a task whose criterion can
//! never run can never close. The submission tool runs each task's
//! criteria in a checkout of the run's tree, with the run's environment,
//! before it accepts the document: a refusal the writer fixes in the
//! same session costs nothing, and the loop's own pre-check later reads
//! the same answers from the cache.
//!
//! A spec is proven the same way, against the plan the run holds: every
//! task it names is the plan's, every file is new to the run's tree — one
//! that is not would replace what the tree holds, and deny it to the work
//! — and in a checkout of that tree with every one of its files written
//! in, each test runs and fails: a test that passes before the work holds
//! the work to nothing.

use std::path::{Path, PathBuf};

use yunta_core::diagnostic::{Diagnostic, Named, Problem, RuleCode, Subject};
use yunta_core::events::ExecutionEnvironment;
use yunta_core::{CommitSha, Spec, SpecFile, Task, TaskId, TasksFile};

use super::session::{RunToolError, SessionTools};
use crate::task_cycle::{probe, CriterionRun};
use crate::worktree::{open_unit, UnitHome, UnitId};

/// What a spec's handover found: every rule it breaks, and how each test
/// of a task the plan declares answered in the run's tree with every
/// file of the spec in it.
pub(super) struct SpecProven {
    pub(super) broken: Vec<Diagnostic>,
    pub(super) failing: Vec<(TaskId, CriterionRun)>,
}

impl SessionTools {
    /// Every rule the document's criteria break where the engine runs
    /// them. Empty when the document can be accepted.
    pub(super) async fn handover(
        &self,
        tasks: &TasksFile,
    ) -> Result<Vec<Diagnostic>, RunToolError> {
        // A task the run already finished, handed over unchanged, keeps
        // its `done`: it will not run again, so it is not checked again.
        let events = self.events().await?;
        let prior = crate::tasks::prior_registrations(&events);
        let current = crate::replay::derive(&events).tasks;
        let (checkout, _) = self.handover_checkout().await?;
        let supervision = self.host.supervision(&self.stop);
        // A plan a gate shows a person says what it changes and why.
        let mut found = match crate::tasks::plan_reviewed(&self.host.workflow, &events, &self.node)
        {
            true => tasks.unexplained(),
            false => Vec::new(),
        };
        for (index, task) in tasks.tasks.iter().enumerate() {
            if crate::tasks::stays_done(task, &prior, &current) {
                continue;
            }
            let probes = probe(task, &checkout, &self.host.memo, supervision)
                .await
                .map_err(|source| RunToolError::Check { source })?;
            found.extend(judged(index, task, &probes, self.host.environment.as_ref()));
        }
        Ok(found)
    }

    /// Every rule the spec breaks against the run's plan and where its
    /// tests run. Empty when the spec can be accepted.
    pub(super) async fn spec_handover(&self, spec: &SpecFile) -> Result<SpecProven, RunToolError> {
        let events = self.events().await?;
        let plan = crate::artifacts::latest::<TasksFile>(&self.host.run_dir, &events)
            .await
            .map_err(|source| RunToolError::Plan { source })?
            .map(|held| held.document);
        let (checkout, base) = self.handover_checkout().await?;
        let supervision = self.host.supervision(&self.stop);
        let broken = already_held(&checkout, &base, spec, supervision).await?;
        write_test_files(&checkout, spec).await?;
        let mut proven = SpecProven {
            broken,
            failing: Vec::new(),
        };
        for (index, one) in spec.specs.iter().enumerate() {
            let (broken, runs) = self.tested(index, one, plan.as_ref(), &checkout).await?;
            proven.broken.extend(broken);
            proven
                .failing
                .extend(runs.into_iter().map(|run| (one.task.clone(), run)));
        }
        Ok(proven)
    }

    /// What one spec breaks where its tests run — a task the plan does
    /// not declare, or a test that cannot run or already passes in
    /// `checkout`, which holds every file of the document — and how each
    /// of its tests answered there.
    async fn tested(
        &self,
        index: usize,
        one: &Spec,
        plan: Option<&TasksFile>,
        checkout: &Path,
    ) -> Result<(Vec<Diagnostic>, Vec<CriterionRun>), RunToolError> {
        let subject = || Subject::Spec(Named::new(one.task.clone(), index));
        let Some(task) = plan.and_then(|plan| plan.tasks.iter().find(|task| task.id == one.task))
        else {
            let unknown = Problem::rule(RuleCode::UnknownSpecTask, unplanned(&one.task, plan));
            return Ok((vec![Diagnostic::new(subject(), unknown)], Vec::new()));
        };
        let tested = Task {
            criteria: one.criteria().collect(),
            depends_on: Vec::new(),
            ..task.clone()
        };
        let supervision = self.host.supervision(&self.stop);
        let probes = probe(&tested, checkout, &self.host.memo, supervision)
            .await
            .map_err(|source| RunToolError::Check { source })?;
        let broken = judged(index, &tested, &probes, self.host.environment.as_ref())
            .into_iter()
            .map(|diagnostic| Diagnostic::new(subject(), diagnostic.problem))
            .collect();
        Ok((broken, probes))
    }

    /// A checkout of the run's tree as it stands, for this node's
    /// handed-over documents alone, and the commit it holds: made once,
    /// and put back to the run's tree before each later submission. What
    /// its builds leave in ignored directories stays, so a second
    /// submission does not pay a cold build again.
    async fn handover_checkout(&self) -> Result<(PathBuf, CommitSha), RunToolError> {
        let supervision = self.host.supervision(&self.stop);
        let base = crate::worktree::head_commit(&self.host.worktree, supervision)
            .await
            .map_err(|source| RunToolError::Handover {
                detail: source.to_string(),
            })?;
        let who = UnitId::Handover(self.node.clone());
        let checkout = crate::run_dir::unit_worktrees(&self.host.run_dir).join(format!("{who}-1"));
        let reset = if checkout.is_dir() {
            crate::git::output(
                &checkout,
                &["reset", "-q", "--hard", base.as_str()],
                supervision,
            )
            .await
            .and(crate::git::output(&checkout, &["clean", "-q", "-fd"], supervision).await)
            .map(|_| ())
            .map_err(|failed| failed.detail())
        } else {
            open_unit(
                UnitHome {
                    repo: &self.host.worktree,
                    run_dir: &self.host.run_dir,
                    run_id: &self.host.run_id,
                    base: &base,
                },
                who,
                1,
                supervision,
            )
            .await
            .map(|_| ())
            .map_err(|failed| failed.to_string())
        };
        reset.map_err(|detail| RunToolError::Handover { detail })?;
        Ok((checkout, base))
    }
}

/// Every file of `spec` whose path the run's tree holds at `base`: written
/// in, it would replace what is there and deny it to the work. Asked of
/// the commit rather than the checkout, whose ignored files an earlier
/// submission may have left.
async fn already_held(
    checkout: &Path,
    base: &CommitSha,
    spec: &SpecFile,
    supervision: crate::process::Supervision<'_>,
) -> Result<Vec<Diagnostic>, RunToolError> {
    let mut found = Vec::new();
    for (index, one) in spec.specs.iter().enumerate() {
        for file in &one.files {
            let at = format!("{}:{}", base.as_str(), file.in_repo());
            let held = crate::git::success(checkout, &["cat-file", "-e", at.as_str()], supervision)
                .await
                .map_err(|failed| RunToolError::Handover {
                    detail: failed.detail(),
                })?;
            if held {
                found.push(Diagnostic::new(
                    Subject::Spec(Named::new(one.task.clone(), index)),
                    Problem::rule(
                        RuleCode::TestFileExists,
                        format!(
                            "`{}` is a file the run's tree already holds; a test lives in a \
                             new file, beside the project's own tests",
                            file.path
                        ),
                    ),
                ));
            }
        }
    }
    Ok(found)
}

/// What one task's probed criteria break. A task with no `depends_on`
/// starts from exactly this tree, so its pre-check is settled here too;
/// one that depends on another only meets its tree after that task's
/// work, so here it answers only for whether its commands can run.
fn judged(
    index: usize,
    task: &Task,
    probes: &[CriterionRun],
    environment: Option<&ExecutionEnvironment>,
) -> Vec<Diagnostic> {
    let independent = task.depends_on.is_empty();
    probes
        .iter()
        .enumerate()
        .filter_map(|(at, run)| {
            let (code, detail) = if run.could_not_run().is_some() {
                (RuleCode::CriterionCannotRun, cannot_run(run, environment))
            } else if independent && !run.is_guard && run.exit_code == 0 {
                (
                    RuleCode::CriterionAlreadyPasses,
                    format!("`{}` already exits 0, before any work", run.cmd),
                )
            } else if independent && run.is_guard && run.exit_code != 0 {
                (
                    RuleCode::GuardAlreadyRed,
                    format!(
                        "`{}` already exits {}, before any work",
                        run.cmd, run.exit_code
                    ),
                )
            } else {
                return None;
            };
            Some(Diagnostic::new(
                Subject::Criterion {
                    task: Named::new(task.id.clone(), index),
                    index: at,
                },
                Problem::rule(code, detail),
            ))
        })
        .collect()
}

/// Why a criterion could not run, in the words a writer fixes it by:
/// what it exited with, what the shell said, and where it looked.
fn cannot_run(run: &CriterionRun, environment: Option<&ExecutionEnvironment>) -> String {
    let mut detail = format!(
        "`{}` exits {} where the engine runs criteria",
        run.cmd,
        run.exit_described()
    );
    if let Some(said) = run.said() {
        detail.push_str(&format!(" — it said `{said}`"));
    }
    if let Some(environment) = environment {
        detail.push_str(&format!(" — under {environment}"));
    }
    detail
}

/// Writes every file `spec` gives its tasks into `checkout`, where its
/// tests run as they will once each task's work starts from them.
async fn write_test_files(checkout: &Path, spec: &SpecFile) -> Result<(), RunToolError> {
    for test_file in spec.specs.iter().flat_map(|one| &one.files) {
        let path = checkout.join(test_file.in_repo());
        let written = match path.parent() {
            Some(parent) => tokio::fs::create_dir_all(parent).await,
            None => Ok(()),
        };
        written
            .and(tokio::fs::write(&path, &test_file.content).await)
            .map_err(|source| RunToolError::Handover {
                detail: format!("could not write `{}`: {source}", test_file.path),
            })?;
    }
    Ok(())
}

/// Why a spec's task is not one the run's plan holds, naming the ones
/// it does.
fn unplanned(task: &yunta_core::TaskId, plan: Option<&TasksFile>) -> String {
    match plan {
        Some(plan) => format!(
            "the run's plan declares no task `{task}`; its tasks are {}",
            yunta_core::text::listed(plan.tasks.iter().map(|task| task.id.as_str()))
        ),
        None => "the run holds no plan for a spec to hold to".to_string(),
    }
}
