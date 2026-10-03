//! The tool a task session declares, on purpose, that its work departs
//! from the plan with — rather than building something else and saying
//! nothing.
//!
//! The declaration names what of the plan it departs from, which the
//! plan must hold: a departure from a shape the plan never declared is
//! a session that misread it, and it is told what the plan does hold.
//! A declared departure is on the log at once, in the session's own
//! words, and the attempt that declared it does not close on it: a
//! person answers first.

use serde::Deserialize;
use serde_json::Value;
use yunta_core::events::{DepartsFrom, DeviationDeclaredPayload, EventPayload, TaskEvent};

use super::catalog::RunTool;
use super::host::TaskAccess;
use super::session::{RunToolError, SessionTools};
use super::verdicts::Reply;

/// What the session declares.
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
        let departure: Departure = serde_json::from_value(Value::Object(args))
            .map_err(|source| RunToolError::InvalidDeparture { source })?;
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
}

/// Refuses a departure from the suite the run holds every task to: it
/// is the run's, measured before any work, and no plan's to depart from.
fn not_the_suite(access: &TaskAccess, from: &DepartsFrom) -> Result<(), RunToolError> {
    match (from, access.suite.as_deref()) {
        (DepartsFrom::Criterion(cmd), Some(suite)) if cmd.trim() == suite.trim() => {
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
