//! `yunta close <run_id>`: closes a stopped run nobody is going to
//! continue, as `cancelled` and by who closed it, so it stops waiting on a
//! person. A run whose engine died is settled first, the way `cancel`
//! settles one. Its checkouts go back to the project's pool; its branches
//! stay for a person to read and for `gc` to remove.

use yunta_core::{Redactor, Responder, RunId};
use yunta_engine::EngineLiveness;

use super::cancel::settle_crash;
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::json::SCHEMA_VERSION;
use crate::output::{emit, Answer, Mode};
use crate::render::blocks::Headline;
use crate::render::doc::{Block, Doc};
use crate::render::ink::Line;
use crate::render::state::RunWord;
use crate::render::{wrap, Look, INDENT};

pub async fn close(
    run_id: &RunId,
    by: Option<&Responder>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.async_storage().await?;
    let open = ctx.open_run(run_id).await?;
    // Resolved before anything is written: a short declared secret refuses
    // the close here, and the run stays as it is.
    let redactor = yunta_engine::run_redactor(&open.manifest.doc, &yunta_core::ProcessSecrets)?;
    let run_dir = open.run_dir.clone();
    let engine = settled_liveness(&ctx, &storage, run_id, &run_dir, &redactor).await?;
    let by = crate::identity::responder(by, ctx.env.user.as_deref());
    yunta_engine::close_run(
        &storage,
        run_id,
        &run_dir,
        &ctx.clock,
        engine,
        by.clone(),
        &redactor,
    )
    .await
    .map_err(|refusal| CliError::close_refused(run_id, refusal))?;
    give_back(&ctx, &open, run_id).await;
    let answer = CloseAnswer {
        schema_version: SCHEMA_VERSION,
        run_id: run_id.clone(),
        word: RunWord::Cancelled,
        by,
    };
    emit(output, &answer, look)
}

/// The close of a run as a document carries it: the word the run
/// stands in, and who closed it.
#[derive(serde::Serialize)]
pub(crate) struct CloseAnswer {
    schema_version: u32,
    run_id: RunId,
    word: RunWord,
    by: Responder,
}

impl Answer for CloseAnswer {
    fn document<'a>(&'a self, look: &Look) -> Doc<'a> {
        let headline = Headline {
            subject: format!("run {}", self.run_id.handle()),
            mark: self.word.mark(),
            said: self.word.to_string(),
        };
        let detail = format!(
            "closed by {} — its checkouts went back to the project, and its branch stays until \
             `yunta gc` removes it",
            self.by
        );
        let lines: Vec<Line> = wrap(&detail, look.width.cells().saturating_sub(INDENT.len()))
            .into_iter()
            .map(|line| Line::new().plain(INDENT).plain(line))
            .collect();
        Doc::new().with(headline).with(Block::Lines(lines))
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}

/// The liveness `close` judges the run by. A dead engine whose registry
/// reads is settled first, so what it left behind is recorded before the
/// close is written.
async fn settled_liveness(
    ctx: &Context,
    storage: &yunta_storage::AsyncStorage,
    run_id: &RunId,
    run_dir: &std::path::Path,
    redactor: &Redactor,
) -> Result<EngineLiveness, CliError> {
    let mut engine = yunta_engine::engine_liveness(run_dir, &yunta_engine::lock::SystemProbe);
    if engine == EngineLiveness::Dead {
        if let yunta_engine::Registry::Read(registry) = yunta_engine::read_registry(run_dir) {
            settle_crash(ctx, storage, run_id, run_dir, &registry.doc, redactor).await?;
            engine = EngineLiveness::Unrecorded;
        }
    }
    Ok(engine)
}

/// Gives a run's checkouts back to the project's pool, as `close` does for a
/// closed run and `cancel` does once a stopped run's groups are gone. A
/// checkout that cannot be given back stays the run's until `gc` collects it.
pub(crate) async fn give_back(ctx: &Context, open: &crate::context::Opened, run_id: &RunId) {
    let manifest = &open.manifest.doc;
    let bound = crate::project::bound_checkout(&yunta_engine::derive(&open.events));
    let tree = ctx
        .project
        .run_tree(manifest, bound.as_deref(), run_id, &ctx.cwd);
    let pool = ctx.project.pool_of(
        manifest,
        &ctx.cwd,
        &open.run_dir,
        std::sync::Arc::new(ctx.clock),
    );
    let supervision = ctx.supervision();
    let units =
        yunta_engine::release_unit_checkouts(&pool, &tree, &open.run_dir, run_id, supervision);
    if let Err(e) = units.await {
        crate::error::warn(format!("run {}: {e}", run_id.handle()));
    }
    if manifest.isolation == yunta_core::Isolation::Worktree {
        let own = yunta_engine::release_run_checkout(&pool, &tree, run_id, supervision);
        if let Err(e) = own.await {
            crate::error::warn(format!("run {}: {e}", run_id.handle()));
        }
    }
}
