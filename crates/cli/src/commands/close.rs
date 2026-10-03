//! `yunta close <run_id>`: closes a stopped run nobody is going to
//! continue, as `cancelled` and by who closed it, so it stops waiting on a
//! person. A run whose engine died is settled first, the way `cancel`
//! settles one. Its branch and worktree stay for a person to read and
//! for `gc` to remove.

use yunta_core::{Responder, RunId};
use yunta_engine::EngineLiveness;

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
    let run_dir = open.run_dir;
    let mut engine = yunta_engine::engine_liveness(&run_dir, &yunta_engine::lock::SystemProbe);
    if engine == EngineLiveness::Dead {
        if let yunta_engine::Registry::Read(registry) = yunta_engine::read_registry(&run_dir) {
            super::cancel::settle_crash(&ctx, &storage, run_id, &run_dir, &registry.doc).await?;
            engine = EngineLiveness::Unrecorded;
        }
    }
    let by = crate::identity::responder(by);
    yunta_engine::close_run(&storage, run_id, &run_dir, &ctx.clock, engine, by.clone())
        .await
        .map_err(|refusal| CliError::close_refused(run_id, refusal))?;
    let word = RunWord::Cancelled;
    let headline = Headline {
        subject: format!("run {called}"),
        mark: word.mark(),
        said: word.to_string(),
    };
    let look = crate::render::stdout_look();
    print!("{}", crate::render::draw(Doc::new().with(headline), &look));
    let detail =
        format!("closed by {by} — its branch and worktree stay until `yunta gc` removes them");
    for line in wrap(&detail, look.width.cells().saturating_sub(INDENT.len())) {
        println!("{INDENT}{line}");
    }
    Ok(Outcome::Success)
}
