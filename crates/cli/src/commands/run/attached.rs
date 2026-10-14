//! `yunta run` without `--detach`: create the run and drive it here,
//! with this invocation watching it to its end — the shape that draws a
//! live view, answers a gate and reports the closing verdict, and the
//! only one a `--adapter mock` fixture can be read in.

use std::path::Path;

use yunta_core::{AdapterId, ModeName};
use yunta_storage::AsyncStorage;

use super::{create_run_from, preflight, runnable};
use crate::commands::drive::{drive, Driving};
use crate::commands::registry::{adapters_for, ScriptedPaths};
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::output::Mode;
use crate::render::Look;

/// What driving a run here needs: the same workflow reference, inputs and
/// overrides every run resolves, plus the fixture that scripts its
/// sessions when there is one and how this invocation reports progress.
pub(super) struct Attaching<'a> {
    pub(super) ctx: &'a Context,
    pub(super) storage: &'a AsyncStorage,
    pub(super) workflow_path: &'a Path,
    pub(super) raw_inputs: &'a [String],
    pub(super) adapter: Option<&'a AdapterId>,
    /// `--adapter mock --fixture <path>`: every session is scripted, and
    /// no real adapter is constructed or probed.
    pub(super) mock_fixture: Option<&'a Path>,
    pub(super) mode: Option<&'a ModeName>,
    /// How this invocation reports: a live view for `Human`, the run id and
    /// nothing else for `Quiet`, one versioned document for `Json`.
    pub(super) output: Mode,
    /// The stdout look `main` built, which the closing block is drawn with.
    pub(super) look: &'a Look,
}

/// Resolves the workflow, freezes its manifest, creates the run and
/// drives it to its verdict, which this function hands back.
pub(super) async fn attached(attaching: Attaching<'_>) -> Result<Outcome, CliError> {
    let Attaching {
        ctx,
        storage,
        workflow_path,
        raw_inputs,
        adapter,
        mock_fixture,
        mode,
        output,
        look,
    } = attaching;

    let frozen = runnable(
        ctx,
        workflow_path,
        raw_inputs,
        adapter,
        mock_fixture,
        &ctx.env,
    )
    .await?;
    let preflight = preflight(ctx, &frozen.manifest, mode, output, look).await?;

    let prepared = create_run_from(ctx, storage, &frozen, mode).await?;
    // The documents the run was born with are on its log now; what the
    // rest of the run needs is the manifest.
    let manifest = frozen.manifest;
    super::super::started(&prepared.run_id, "created", output, look);

    let adapters = adapters_for(
        &manifest,
        &ScriptedPaths {
            run_dir: &prepared.run_dir,
            worktree: &prepared.worktree,
        },
    )?;
    drive(Driving {
        ctx,
        storage,
        manifest: &manifest,
        prepared: &prepared,
        adapters,
        prior: preflight.prior,
        warnings: preflight.warnings,
        output,
        look,
    })
    .await
}
