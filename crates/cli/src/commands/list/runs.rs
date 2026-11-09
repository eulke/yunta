//! `yunta list --runs`, and `yunta` alone in a repository: the field —
//! this repository's runs, or every run on the machine — gathered by the
//! [`Inbox`] and drawn as render's field page.

use surco::doc::Depth;
use surco::look::Look;
use yunta_render::page::{Field, Page};

use super::inbox::Inbox;
use crate::context::Context;
use crate::error::CliError;
use crate::json::SCHEMA_VERSION;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};

/// The runs of the repository this is run in — every run on the machine
/// with `all`, or when this is run outside a repository — as the field,
/// as deep as `depth` asks.
pub async fn list_runs(
    all: bool,
    depth: Depth,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let inbox = Inbox::gather(&ctx, all).await?;
    let answer = ListRunsAnswer {
        schema_version: SCHEMA_VERSION,
        storage: ctx.project.storage_path.clone(),
        inbox,
        depth,
    };
    emit(output, &answer, look)
}

/// The listing as a document carries it: the runs it is about, each read
/// off its own frame, and where this project keeps its runs, which a
/// listing with none names.
#[derive(serde::Serialize)]
pub(crate) struct ListRunsAnswer {
    schema_version: u32,
    storage: std::path::PathBuf,
    #[serde(flatten)]
    inbox: Inbox,
    #[serde(skip)]
    depth: Depth,
}

impl Answer for ListRunsAnswer {
    fn page(&self) -> impl Page + '_ {
        Field {
            rows: &self.inbox.rows,
            unreadable: &self.inbox.unreadable,
            elsewhere: self.inbox.elsewhere,
            here: self.inbox.here,
            storage: &self.storage,
            depth: self.depth,
        }
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}
