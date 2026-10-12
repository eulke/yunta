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
use crate::render::blocks::Headline;
use crate::render::doc::Doc;
use crate::render::state::RunWord;
use crate::render::{wrap, INDENT};

pub async fn close(run_id: &RunId, by: Option<&Responder>) -> Result<Outcome, CliError> {
    let called = run_id.handle();
    let ctx = Context::load()?;
    let storage = ctx.async_storage().await?;
    let open = ctx.open_run(run_id).await?;
    // Resolved before anything is written: a short declared secret refuses
    // the close here, and the run stays as it is.
    let redactor = yunta_engine::run_redactor(&open.manifest.doc, &yunta_core::ProcessSecrets)?;
    let run_dir = open.run_dir.clone();
    let engine = settled_liveness(&ctx, &storage, run_id, &run_dir, &redactor).await?;
    let by = crate::identity::responder(by);
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
    let word = RunWord::Cancelled;
    let headline = Headline {
        subject: format!("run {called}"),
        mark: word.mark(),
        said: word.to_string(),
    };
    let look = crate::render::stdout_look();
    print!("{}", crate::render::draw(Doc::new().with(headline), &look));
    let detail = format!(
        "closed by {by} — its checkouts went back to the project, and its branch stays until \
         `yunta gc` removes it"
    );
    for line in wrap(&detail, look.width.cells().saturating_sub(INDENT.len())) {
        println!("{INDENT}{line}");
    }
    Ok(Outcome::Success)
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

/// Gives a closed run's checkouts back to the project's pool: nothing will
/// continue in them. One that cannot be given back stays the run's until
/// `gc` collects it.
async fn give_back(ctx: &Context, open: &crate::context::Opened, run_id: &RunId) {
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
