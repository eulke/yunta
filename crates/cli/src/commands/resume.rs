//! `yunta resume <run_id>`: take a parked run back off its log and drive it
//! forward with the same drive that started it — the log decides what
//! remains.
//!
//! run.dir is looked up in search order (current config's runs root, then
//! the default) — and once the manifest is open, the worktree comes from its
//! *frozen* paths, so a `paths.*` change between `run` and `resume` never
//! loses the run.
//!
//! What a person watching sees is what `yunta run` shows, down to the flags:
//! two commands that execute the same thing have no business reporting it
//! differently.
//!
//! A run has one engine: the API refuses a resume while another engine
//! drives the run, before it waits for anything, and claims the run before it
//! takes a checkout.

use surco::look::Look;
use yunta_api::Remark;
use yunta_core::RunId;

use crate::commands::drive::{drive, Driving};
use crate::context::Context;
use crate::error::CliError;
use crate::output::Mode;
use crate::output::Outcome;
use crate::shell::Shell;

pub async fn resume(
    shell: &Shell,
    run_id: &RunId,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::of(shell)?;
    let open = ctx.project.run(run_id).await?;
    let say = |remark: Remark| crate::remarks::say(remark, output, look);
    let Some(held) = open.take_back(&say, &ctx.stop()).await? else {
        // A person interrupted the wait for a reset or for the engine that
        // parked the run: it stays parked, and nothing was taken.
        return Ok(crate::output::Outcome::interrupted());
    };
    super::started(run_id, "resuming", output, look);
    drive(
        Driving {
            ctx: &ctx,
            output,
            look,
        },
        held,
    )
    .await
}
