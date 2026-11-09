//! `yunta run` without `--detach`: create the run and drive it here,
//! with this invocation watching it to its end — the shape that draws a
//! live view, answers a gate and reports the closing verdict, and the
//! only one a `--adapter mock` fixture can be read in.

use std::path::Path;

use surco::look::Look;
use surco::words::Standing;
use yunta_core::{AdapterId, ModeName};

use super::{admitted, create_run_from, Asked};
use crate::commands::drive::{drive, Driving};
use crate::context::Context;
use crate::error::CliError;
use crate::output::Mode;
use crate::output::Outcome;

/// What driving a run here needs: the same workflow reference, inputs and
/// overrides every run resolves, plus the fixture that scripts its
/// sessions when there is one and how this invocation reports progress.
pub(super) struct Attaching<'a> {
    pub(super) ctx: &'a Context,
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

/// Admits the run, creates it, holds it and drives it to its verdict, which
/// this function hands back.
pub(super) async fn attached(attaching: Attaching<'_>) -> Result<Outcome, CliError> {
    let Attaching {
        ctx,
        workflow_path,
        raw_inputs,
        adapter,
        mock_fixture,
        mode,
        output,
        look,
    } = attaching;
    let asked = Asked {
        workflow_path,
        raw_inputs,
        adapter,
        mock_fixture,
        mode,
        hermetic: false,
    };
    let admission = admitted(ctx, asked, output, look).await?;
    let created = create_run_from(ctx, admission, output, look).await?;
    super::super::started(&created.id, Standing::Created.word(), output, look);
    let held = created.hold(&ctx.stop()).await?;
    drive(Driving { ctx, output, look }, held).await
}
