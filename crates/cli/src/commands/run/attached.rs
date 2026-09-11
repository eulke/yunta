//! `yunta run` without `--detach`: create the run and drive it here,
//! with this invocation watching it to its end — the shape that draws a
//! live view, answers a gate and reports the closing verdict, and the
//! only one a `--adapter mock` fixture can be read in.

use std::path::Path;

use yunta_adapters::MOCK_ID;
use yunta_core::{AdapterId, ModeName};
use yunta_storage::AsyncStorage;

use super::{create_run_from, estimate, runnable};
use crate::commands::drive::{drive, Driving};
use crate::commands::test::{load_mock_fixture, mock_adapters};
use crate::context::Context;
use crate::error::{CliError, Outcome};

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
    /// `--quiet`: the run id and nothing else on stdout.
    pub(super) quiet: bool,
    /// `--json`: one versioned document on stdout and nothing else.
    pub(super) json: bool,
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
        quiet,
        json,
    } = attaching;

    let (manifest, real_adapters) =
        runnable(ctx, workflow_path, raw_inputs, adapter, mock_fixture).await?;
    let prior = estimate(ctx, storage, &manifest, quiet, json).await;

    let prepared = create_run_from(ctx, storage, &manifest, mode).await?;
    if !json {
        println!(
            "run {}: created at {}",
            prepared.run_id,
            prepared.run_dir.display()
        );
    }

    let adapters = match mock_fixture {
        Some(path) => {
            let mock = load_mock_fixture(path, &prepared.run_dir, &prepared.worktree)
                .map_err(CliError::msg)?;
            mock_adapters(&ctx.project.config, mock)
        }
        None => real_adapters,
    };
    drive(Driving {
        ctx,
        storage,
        manifest: &manifest,
        prepared: &prepared,
        adapters,
        adapter_override: adapter.filter(|id| **id != MOCK_ID).cloned(),
        prior,
        quiet,
        json,
    })
    .await
}
