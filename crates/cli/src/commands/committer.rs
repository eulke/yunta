//! Who a run's commits are made as, asked of git before the run exists.

use crate::context::Context;
use crate::error::CliError;

/// Who git commits as in this repository, or what to do because it
/// cannot name anyone: the sentence a refused run and `doctor` share.
pub(crate) async fn committer(ctx: &Context) -> Result<String, String> {
    match yunta_engine::git::committer(&ctx.cwd, ctx.supervision()).await {
        Ok(who) => Ok(who),
        Err(stopped) if stopped.cancelled() => Err(stopped.to_string()),
        Err(unnamed) => Err(format!(
            "cannot name who commits here: set `git config --global user.name \"Your Name\"` \
             and `git config --global user.email you@example.com` (without `--global`, for \
             this repository alone) — git said: {}",
            unnamed.detail().lines().last().unwrap_or_default()
        )),
    }
}

/// Refuses a run in a repository git cannot commit in. Every node's work
/// becomes a commit, so such a run would fail at its first one, after
/// spending on the node before it.
pub(crate) async fn refuse_without_committer(ctx: &Context) -> Result<(), CliError> {
    committer(ctx)
        .await
        .map(drop)
        .map_err(|why| CliError::msg(format!("a run commits every node's work, and git {why}")))
}
