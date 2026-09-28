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

use std::path::PathBuf;

use yunta_core::diagnostic::{Diagnostic, Named, Problem, RuleCode, Subject};
use yunta_core::events::ExecutionEnvironment;
use yunta_core::{Task, TasksFile};

use super::session::{RunToolError, SessionTools};
use crate::task_cycle::{probe, CriterionRun};
use crate::worktree::{open_unit, UnitHome, UnitId};

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
        let checkout = self.handover_checkout().await?;
        let supervision = self.host.supervision(&self.stop);
        let mut found = Vec::new();
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

    /// A checkout of the run's tree as it stands, for this node's
    /// handed-over documents alone: made once, and put back to the run's
    /// tree before each later submission. What its builds leave in
    /// ignored directories stays, so a second submission does not pay a
    /// cold build again.
    async fn handover_checkout(&self) -> Result<PathBuf, RunToolError> {
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
        Ok(checkout)
    }
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
