//! `yunta close <run_id>`: closes a stopped run nobody is going to
//! continue, through the API, as `cancelled` and by who closed it, so it
//! stops waiting on a person; this command says what the close says while it
//! works, and answers who closed it.

use surco::look::Look;
use surco::words::Standing;
use yunta_api::Remark;
use yunta_core::{Responder, RunId};
use yunta_render::document::CloseDocument;
use yunta_render::page::{Closed, Page};

use crate::context::Context;
use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use crate::shell::Shell;

pub async fn close(
    shell: &Shell,
    run_id: &RunId,
    by: Option<&Responder>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::of(shell)?;
    let open = ctx.project.run(run_id).await?;
    let say = |remark: Remark| crate::remarks::say(remark, output, look);
    let by = open.close(by, &say, &ctx.stop()).await?;
    let answer = CloseAnswer {
        run_id: run_id.clone(),
        word: Standing::Stopped,
        by,
    };
    emit(output, &answer, look)
}

/// The close of a run: the word the run stands in, and who closed it.
pub(crate) struct CloseAnswer {
    run_id: RunId,
    word: Standing,
    by: Responder,
}

impl serde::Serialize for CloseAnswer {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let document = CloseDocument {
            run_id: &self.run_id,
            word: self.word,
            by: &self.by,
        };
        serde::Serialize::serialize(&document, serializer)
    }
}

impl Answer for CloseAnswer {
    fn page(&self) -> impl Page + '_ {
        Closed {
            run_id: &self.run_id,
            word: self.word,
            by: &self.by,
        }
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}
