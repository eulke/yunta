//! See [`super`]. The work the catalog asks for on a workflow: a check, the
//! repository's cases, and a new workflow; and what a workflow is read as, its past
//! runs and its graph; each as its command does it.

use std::path::Path;

use yunta_api::{Error, Project, Remarks, Stop, WorkflowStem};
use yunta_core::{WorkflowName, WorkflowShape};
use yunta_render::page::Graph;

use crate::{NewWorkflowWritten, News, Outcome, WorkflowOperation, WorkflowOutcome};

/// What `work` comes to, through the API: what it says said through `remarks`, and
/// what the cases hand back as they end handed to `found`.
pub(super) async fn performed(
    project: &Project,
    work: WorkflowOperation,
    remarks: &dyn Remarks,
    found: &(dyn Fn(News) + Sync),
    stop: &Stop,
) -> Result<Outcome, Error> {
    let outcome = match work {
        WorkflowOperation::Check { workflow } => {
            let check = project.check(Path::new(&workflow), None, stop).await?;
            WorkflowOutcome::Checked(Box::new(check))
        }
        WorkflowOperation::RunCases => cases_run(project, remarks, found, stop).await?,
        WorkflowOperation::New { name, shape } => {
            new_workflow_written(project, name, shape, stop).await?
        }
    };
    Ok(Outcome::Workflow(outcome))
}

/// The past runs of the workflow called `workflow`, and what their logs say of its
/// checks, as `yunta stats --workflow` reads them.
pub(super) async fn history_read(
    project: &Project,
    workflow: &WorkflowName,
) -> Result<Outcome, Error> {
    let reading = project.history(workflow).await.reading();
    Ok(Outcome::History(Box::new(reading)))
}

/// The workflow the catalog offers as `workflow`, read as `yunta graph` reads it, and
/// drawn as written.
pub(super) fn graph_read(project: &Project, workflow: &str) -> Result<Outcome, Error> {
    let read = project.workspace.read_workflow(Path::new(workflow))?;
    let graph = Graph {
        workflow: read.workflow,
        run: None,
    };
    Ok(Outcome::Graph(Box::new(graph)))
}

/// The repository's cases run in turn, as `yunta test` runs them: every case's
/// name handed to `found` before the first runs, each case as it ends, what their
/// runs say through `remarks`, and the tally last. The stop stops the case running,
/// and the ones after it as they start.
async fn cases_run(
    project: &Project,
    remarks: &dyn Remarks,
    found: &(dyn Fn(News) + Sync),
    stop: &Stop,
) -> Result<WorkflowOutcome, Error> {
    let suite = project.workspace.cases(&project.workspace.dir)?;
    found(News::CasesNamed(suite.names()));
    let tested = suite
        .run(|case| found(News::Case(case)), remarks, stop)
        .await;
    Ok(WorkflowOutcome::Tested(tested))
}

/// The workflow `name` written from the skeleton of `shape` and checked, as
/// `yunta new` writes it: refused where the name is no file's stem or the file is
/// there already. What stops its check is said beside it, written whatever did.
async fn new_workflow_written(
    project: &Project,
    name: String,
    shape: WorkflowShape,
    stop: &Stop,
) -> Result<WorkflowOutcome, Error> {
    let stem = WorkflowStem::parse(&name)?;
    let written = project.workspace.new_workflow(&stem, shape, false).await?;
    let diagnosis = written
        .diagnosis(stop)
        .await
        .map_err(|unjudged| unjudged.to_string());
    Ok(WorkflowOutcome::Written(Box::new(NewWorkflowWritten {
        at: project.workspace.shown(&written.path),
        name,
        shape,
        path: written.path.clone(),
        diagnosis,
    })))
}
