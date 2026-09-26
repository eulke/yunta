//! `yunta resume <run_id>`: reload the frozen manifest from run.dir and
//! drive the run forward with the same `execute_run` that started it —
//! the log decides what remains.
//!
//! run.dir is looked up in search order (current config's runs root,
//! then the default) — and once the manifest is open, the worktree
//! comes from its *frozen* paths, so a `paths.*` change between `run`
//! and `resume` never loses the run.
//!
//! What a person watching sees is what `yunta run` shows, down to the
//! flags: two commands that execute the same thing have no business
//! reporting it differently.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use yunta_core::{AdapterId, Isolation, Manifest, RunId};

use crate::commands::drive::{drive, Driving, Prepared};
use crate::context::Context;
use crate::error::{CliError, Outcome};

/// A run this invocation is about to pick up, as its own frozen manifest
/// describes it: where its state lives, what it runs on, and the tree it
/// works in.
struct Parked {
    run_dir: PathBuf,
    manifest: Manifest,
    worktree: PathBuf,
    adapters: HashMap<AdapterId, Arc<dyn yunta_core::port::Adapter>>,
}

/// Opens the run `run_id` names, refusing one this project cannot reach
/// or this binary cannot run.
///
/// Everything comes from the manifest the run froze, never the project's
/// current config: a run never re-reads config after it is created, and
/// the worktree it works in is the one its own frozen roots name.
async fn parked(ctx: &Context, run_id: &RunId) -> Result<Parked, CliError> {
    let open = ctx.open_run(run_id).await?;
    let (run_dir, manifest) = (open.run_dir, open.manifest.doc);
    let adapters = super::real_adapters(&manifest.config);
    super::refuse_unrunnable(&manifest.workflow, &adapters)?;
    let worktree = match manifest.isolation {
        Isolation::Worktree => ctx
            .project
            .worktrees_root_for(&manifest)
            .join(run_id.as_str()),
        Isolation::None => ctx.cwd.clone(),
    };
    Ok(Parked {
        run_dir,
        manifest,
        worktree,
        adapters,
    })
}

pub async fn resume(run_id: &RunId, quiet: bool, json: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let Parked {
        run_dir,
        manifest,
        worktree,
        adapters,
    } = parked(&ctx, run_id).await?;
    let storage = ctx.async_storage().await?;
    if !json {
        println!("run {run_id}: resuming at {}", run_dir.display());
    }
    let prepared = Prepared {
        run_id: run_id.clone(),
        run_dir,
        worktree,
    };
    drive(Driving {
        ctx: &ctx,
        storage: &storage,
        manifest: &manifest,
        prepared: &prepared,
        adapters,
        // A resume never overrides the runners the run froze, and it
        // reports what this run did rather than what a catalog of past
        // runs cost: the history belongs to the invocation that chose to
        // start one.
        adapter_override: None,
        prior: None,
        // §8.6 gives the pre-run estimation to whoever creates a run;
        // this picks one up.
        warnings: Default::default(),
        quiet,
        json,
    })
    .await
}
