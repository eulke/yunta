//! The tool a task session declares with, on purpose, when its work cannot
//! follow the plan — rather than building something else and saying
//! nothing — or when one of its criteria cannot pass on any work the plan
//! allows.
//!
//! A departure names what of the plan it departs from, which the plan must
//! hold: a departure from a shape the plan never declared is a session that
//! misread it, and it is told what the plan does hold. A declared departure
//! is on the log at once, in the session's own words, and the attempt that
//! declared it does not close on it: a person answers first.
//!
//! An unpassable declaration names one of the task's own criteria. The
//! engine runs it on the work: a criterion that passes, or that the
//! environment could not run, is refused. A criterion that is red is
//! recorded with the exit it gave, and the attempt ends with it: the plan
//! answers for the criterion.

use serde::Deserialize;
use serde_json::Value;
use yunta_core::events::{
    CriterionUnpassablePayload, DepartsFrom, DeviationDeclaredPayload, EventPayload, TaskEvent,
};
use yunta_core::expansion::Precheck;

use super::catalog::RunTool;
use super::host::TaskAccess;
use super::session::{RunToolError, SessionTools};
use super::verdicts::{self, Reply};
use crate::scope_expansion::run_criterion;

/// What the session declares, by its `kind`.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Declaration {
    Departure(Departure),
    Unpassable { criterion: String, why: String },
}

/// A departure from the plan, in the session's own words.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Departure {
    from: DepartsFrom,
    planned: String,
    instead: String,
    why: String,
}

impl SessionTools {
    pub(super) async fn declare_deviation(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let access = self.task_access(RunTool::DeclareDeviation)?;
        let declaration: Declaration = serde_json::from_value(Value::Object(args))
            .map_err(|source| RunToolError::InvalidDeparture { source })?;
        match declaration {
            Declaration::Departure(departure) => self.declare_departure(access, departure).await,
            Declaration::Unpassable { criterion, why } => {
                self.declare_unpassable(access, criterion, why).await
            }
        }
    }

    /// Records a departure from the plan, which the attempt does not close on.
    async fn declare_departure(
        &self,
        access: &TaskAccess,
        departure: Departure,
    ) -> Result<String, RunToolError> {
        for (field, text) in [
            ("planned", &departure.planned),
            ("instead", &departure.instead),
            ("why", &departure.why),
        ] {
            if text.trim().is_empty() {
                return Err(RunToolError::EmptyDeparture { field });
            }
        }
        held_by_the_plan(access, &departure.from)?;
        not_the_suite(access, &departure.from)?;
        let declared = DeviationDeclaredPayload {
            task_id: access.task.id.clone(),
            from: departure.from,
            planned: departure.planned,
            instead: departure.instead,
            why: departure.why,
        };
        self.append(EventPayload::Tasks(TaskEvent::DeviationDeclared(declared)))
            .await?;
        Ok(Reply::new(
            "departure recorded — your task does not close on it: when this session ends a \
             person accepts it or sends it back with what to do instead",
        )
        .next(format!(
            "finish the rest of the task as the plan says; `{}` tells you whether it closes",
            self.called(RunTool::CheckTask)
        ))
        .text())
    }

    /// Records that `criterion` is red on the work and that no work of the
    /// plan can pass it. A session declares one criterion at most: the first
    /// declaration stands, and a second is answered without being recorded.
    async fn declare_unpassable(
        &self,
        access: &TaskAccess,
        criterion: String,
        why: String,
    ) -> Result<String, RunToolError> {
        if why.trim().is_empty() {
            return Err(RunToolError::EmptyDeparture { field: "why" });
        }
        let from = DepartsFrom::Criterion(criterion.clone());
        held_by_the_plan(access, &from)?;
        not_the_suite(access, &from)?;
        let precheck = run_criterion(
            &access.unit.worktree,
            &criterion,
            self.host.supervision(&self.stop),
        )
        .await
        .map_err(|source| RunToolError::Criterion { source })?;
        let declared = CriterionUnpassablePayload {
            task_id: access.task.id.clone(),
            exit_code: red_exit(&criterion, precheck)?,
            criterion,
            why,
        };
        let next = format!(
            "end this attempt; the plan, not more work on the task, answers for `{}`",
            declared.criterion
        );
        // The check and the append share one lock: a second declaration of the
        // session is answered, and never recorded.
        let mut first = access.declared.lock().await;
        if first.is_some() {
            return Ok(verdicts::already_declared(
                "end this attempt; the declaration already made stands",
            ));
        }
        self.append(EventPayload::Tasks(TaskEvent::CriterionDeclaredUnpassable(
            declared.clone(),
        )))
        .await?;
        *first = Some(declared);
        Ok(verdicts::declared(&next))
    }
}

/// The exit a declared criterion records, when the criterion is red on the
/// work. A criterion run here always answers, so its exit is known. A pass
/// and a command the engine stopped before it answered are refused with the
/// exit the run records for them. So are the exits 126 and 127, which the
/// shell gives when it cannot run or find a command: those name the
/// environment, not the work.
fn red_exit(criterion: &str, precheck: Precheck) -> Result<i32, RunToolError> {
    match precheck {
        Precheck::Red { exit } if !matches!(exit, 126 | 127) => Ok(exit),
        answered => Err(RunToolError::NotRedOnTheWork {
            criterion: criterion.to_string(),
            exit: answered.exit_code().unwrap_or_default(),
        }),
    }
}

/// Refuses a departure from a command the run holds every task to: it is
/// the run's, measured before any work, and no plan's to depart from.
fn not_the_suite(access: &TaskAccess, from: &DepartsFrom) -> Result<(), RunToolError> {
    match from {
        DepartsFrom::Criterion(cmd) if access.held.iter().any(|held| held.trim() == cmd.trim()) => {
            Err(RunToolError::SuiteDeparture { cmd: cmd.clone() })
        }
        _ => Ok(()),
    }
}

/// Refuses a departure from something the plan does not hold, naming
/// what it does.
fn held_by_the_plan(access: &TaskAccess, from: &DepartsFrom) -> Result<(), RunToolError> {
    let task = &access.task;
    let plan = access.plan.as_deref();
    let (held, known): (bool, Vec<&str>) = match from {
        DepartsFrom::Shape(name) => {
            let names: Vec<&str> = plan
                .map(|plan| plan.shapes.iter().map(|s| s.name.as_str()).collect())
                .unwrap_or_default();
            (names.contains(&name.as_str()), names)
        }
        DepartsFrom::Decision(id) => {
            let ids: Vec<&str> = plan
                .map(|plan| plan.decisions.iter().map(|d| d.id.as_str()).collect())
                .unwrap_or_default();
            (ids.contains(&id.as_str()), ids)
        }
        DepartsFrom::Change(at) => {
            let places: Vec<&str> = task.changes.iter().map(|c| c.at.as_str()).collect();
            (places.contains(&at.as_str()), places)
        }
        DepartsFrom::Criterion(cmd) => {
            let cmds: Vec<&str> = task.criteria.iter().map(|c| c.cmd.as_str()).collect();
            (cmds.contains(&cmd.as_str()), cmds)
        }
        DepartsFrom::Outcome => (task.outcome.is_some(), Vec::new()),
    };
    match held {
        true => Ok(()),
        false => Err(RunToolError::NotInThePlan {
            from: from.to_string(),
            known: match known.as_slice() {
                [] => "nothing of that kind".to_string(),
                _ => yunta_core::text::listed(known.iter().copied()),
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_criterion_red_on_the_work_is_declared_with_its_exit() {
        assert_eq!(red_exit("false", Precheck::Red { exit: 1 }).ok(), Some(1));
        assert!(matches!(
            red_exit("true", Precheck::Passes),
            Err(RunToolError::NotRedOnTheWork { exit: 0, .. })
        ));
        assert!(matches!(
            red_exit("cmd", Precheck::Red { exit: 127 }),
            Err(RunToolError::NotRedOnTheWork { exit: 127, .. })
        ));
        // The exit the run records for a command it stopped before it answered.
        assert!(matches!(
            red_exit("cmd", Precheck::CouldNotRun),
            Err(RunToolError::NotRedOnTheWork { exit: -2, .. })
        ));
    }
}
