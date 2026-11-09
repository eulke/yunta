//! `yunta run --detach`: create the run, hand it to a fully independent
//! `yunta resume`, and report its id without waiting — the shape the MCP
//! control plane's `run_workflow` reaches a run in too, so neither ever
//! blocks for a run's duration.

use std::path::Path;

use surco::look::Look;
use yunta_core::{AdapterId, ModeName, RunId};

use super::{admitted, create_run_from, Asked};
use crate::commands::started;
use crate::context::Context;
use crate::error::CliError;
use crate::output::Mode;
use crate::output::Outcome;

/// What a detached start needs to reach a run of its own: the same
/// workflow reference, inputs and overrides every run resolves, plus how
/// this invocation reports what it made.
pub(super) struct Detaching<'a> {
    pub(super) ctx: &'a Context,
    pub(super) workflow_path: &'a Path,
    pub(super) raw_inputs: &'a [String],
    pub(super) adapter: Option<&'a AdapterId>,
    pub(super) mode: Option<&'a ModeName>,
    /// How this invocation reports the run it made: the run id and nothing
    /// else for `Quiet`, one versioned document for `Json`. The budget
    /// caution, which asks for a decision, also goes out on stderr.
    pub(super) output: Mode,
    /// The stdout look `main` built, which the estimation line is drawn with.
    pub(super) look: &'a Look,
}

/// Creates the run, hands it to a detached `yunta resume`, and reports
/// its id without waiting on it.
///
/// The estimation happens here, between freezing the manifest and
/// creating the run, because this invocation is the only one that can
/// carry it: the distribution — and the budget warning derived from it —
/// is said to whoever *creates* a run, and a `yunta resume`, detached or
/// not, picks one up instead. A warning that asks whether a cap is worth
/// starting under is worth nothing once the child is already spending, so
/// it goes out before the child exists.
pub(super) async fn detached(detaching: Detaching<'_>) -> Result<Outcome, CliError> {
    let Detaching {
        ctx,
        workflow_path,
        raw_inputs,
        adapter,
        mode,
        output,
        look,
    } = detaching;
    // No fixture ever reaches here: a detached child resolves the
    // adapters `runners:` names and reads none.
    let asked = Asked {
        workflow_path,
        raw_inputs,
        adapter,
        mock_fixture: None,
        mode,
        hermetic: false,
    };
    let admission = admitted(ctx, asked, output, look).await?;
    let created = create_run_from(ctx, admission, output, look).await?;
    let (manifest, warnings) = (created.manifest.clone(), created.warnings.clone());
    let run_id = created.start_detached(&ctx.stop()).await?;
    if output == Mode::Json {
        // The run's own log, read the instant it was handed off: the
        // same document `yunta status --json` prints, saying where the
        // run stood when this invocation let go of it. The verdict is
        // the handoff's and not the run's — a run still moving is what
        // this command set out to leave behind.
        let document = ctx
            .project
            .run_document(&run_id, &manifest, warnings)
            .await?;
        crate::output::print_json(&document)?;
        return Ok(Outcome::Success);
    }
    started(
        &run_id,
        "detached, driving forward independently",
        output,
        look,
    );
    Ok(Outcome::Success)
}

/// Creates a run for a real (non-mock) workflow and hands it to a detached
/// `yunta resume`, returning its id without waiting — the control plane's
/// `run_workflow`, admitted, created and handed off through the same API
/// `yunta run --detach` uses, so both reach a run identically.
///
/// Says nothing about what this workflow has cost before: an agent client
/// reads the run id this returns, and that client gets the same
/// distribution through `list_workflows` — the surface it consults while it
/// is still choosing a workflow.
pub(crate) async fn start_detached(
    ctx: &Context,
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    mode: Option<&ModeName>,
) -> Result<Started, CliError> {
    // Quiet, because stdout here *is* the control plane's JSON-RPC
    // stream: the distribution line the estimation prints for a person
    // would land in the middle of a response. What it has to say
    // travels in the answer instead. Quiet draws nothing, so the look
    // only completes the call.
    let look = crate::output::stdout_look();
    let asked = Asked {
        workflow_path,
        raw_inputs,
        adapter,
        mock_fixture: None,
        mode,
        hermetic: false,
    };
    let admission = admitted(ctx, asked, Mode::Quiet, &look).await?;
    let created = create_run_from(ctx, admission, Mode::Quiet, &look).await?;
    let warnings = created.warnings.clone();
    let run_id = created.start_detached(&ctx.stop()).await?;
    Ok(Started { run_id, warnings })
}

/// A run that was created and handed off, and what the caller owes its
/// reader about it.
pub(crate) struct Started {
    pub(crate) run_id: RunId,
    /// What was said before the first token: the budget warning, when this
    /// workflow's history has one to give, and the `files:` a node reads
    /// that the run would not find. A client that starts runs is the one
    /// deciding whether the run is worth starting, and it never sees
    /// stderr.
    pub(crate) warnings: yunta_render::document::PreRunWarnings,
}
