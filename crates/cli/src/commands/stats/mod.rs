//! `yunta stats <run_id>` and `yunta stats --workflow <name>`: every
//! number comes from replay's pure derivation over the event log, which
//! the API reads off disk — this module only renders them, either as `--json` or as a
//! terminal visualization (horizontal bars per node/runner, a sparkline
//! of a workflow's historical CPTV, a comparison table between modes).
//! Every rendered line stays inside the width stdout gets and reads
//! with its glyphs and color stripped, because the columns, bars,
//! words and glyphs come from `crate::output` and `yunta_render`, and the
//! width and the glyphs are settled once in `crate::output`, where both
//! rules live.

use yunta_render::page::{History, Page, Stats};

use yunta_core::{RunId, WorkflowName};

use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use crate::shell::Shell;
use surco::doc::Depth;
use surco::look::Look;
use yunta_render::document::stats::{RunStatsJson, WorkflowHistoryJson};

/// What `yunta stats <run_id>` answers: the run's document for a program,
/// and its block for a person.
#[derive(serde::Serialize)]
struct RunStatsAnswer<'a> {
    #[serde(flatten)]
    json: RunStatsJson,
    #[serde(skip)]
    page: Stats<'a>,
}

impl Answer for RunStatsAnswer<'_> {
    fn page(&self) -> impl Page + '_ {
        &self.page
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}

/// Prints what the run `run_id` spent, at `depth`, as the document `output` asks for.
pub(crate) async fn of_run(
    shell: &Shell,
    run_id: &RunId,
    depth: Depth,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    // The page reads the run at one instant, its headline's.
    let spent = project.run(run_id).await?.stats();
    let pricing = spent.pricing.as_ref();
    let mode = spent.frame.mode.as_str();
    let answer = RunStatsAnswer {
        json: RunStatsJson::from(run_id, mode, &spent.stats, pricing),
        page: Stats {
            frame: &spent.frame,
            stats: &spent.stats,
            pricing,
            depth,
        },
    };
    emit(output, &answer, look)
}

/// What `yunta stats --workflow <name>` answers: the history as a document
/// for a program, and as the page a person reads.
#[derive(serde::Serialize)]
struct WorkflowAnswer<'a> {
    #[serde(flatten)]
    json: WorkflowHistoryJson,
    #[serde(skip)]
    page: History<'a>,
}

impl Answer for WorkflowAnswer<'_> {
    fn page(&self) -> impl Page + '_ {
        &self.page
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}

/// Prints what the history of a workflow shows, as the document `output` asks for.
pub(crate) async fn of_workflow(
    shell: &Shell,
    workflow_name: &WorkflowName,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;

    let past = project.history(workflow_name).await.reading();
    let findings = past.findings.as_ref();
    let answer = WorkflowAnswer {
        json: WorkflowHistoryJson::from(workflow_name, &past.summaries, findings),
        page: History {
            name: workflow_name,
            history: &past.summaries,
            findings,
        },
    };
    emit(output, &answer, look)
}
