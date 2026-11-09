//! `yunta list`: the repo's own catalog of workflows — name,
//! description, declared inputs — with `--runs` switching to local runs
//! instead. Both answer the same question without a server: "what's
//! here, and where does it stand." The API reads both; this draws them.
//!
//! The runs view lives in [`runs`], which draws the field the API reads.

mod runs;

use yunta_render::page::Signpost;

use crate::error::CliError;
use crate::output::show;
use crate::output::Outcome;
use crate::shell::Shell;
use surco::doc::Depth;
use surco::look::Look;

pub use runs::list_runs;

/// `yunta` alone: in a repository, its field — the runs of that
/// repository; anywhere else, where Yunta works and what to type.
pub async fn field(shell: &Shell, look: &Look) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    if project.in_a_repository(&shell.stop()).await? {
        return list_runs(
            shell,
            yunta_api::FieldReach::Here,
            Depth::Glance,
            crate::output::Mode::Human,
            look,
        )
        .await;
    }
    show(look, &Signpost);
    Ok(Outcome::Success)
}

/// `yunta list --workflows`: the catalog the API reads here.
pub async fn list_workflows(shell: &Shell, look: &Look) -> Result<Outcome, CliError> {
    show(look, &shell.workspace().catalog().await);
    Ok(Outcome::Success)
}
