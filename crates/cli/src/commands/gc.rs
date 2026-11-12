//! `yunta gc`: collects terminal runs once `storage.retention_days` has
//! passed since their last event, and tidies the project's pool, through the
//! API. Each thing the pass could not do is warned as it happens; what the
//! pass did is one page, said once it is over.

use surco::look::Look;
use yunta_api::{Collect, Remark};

use crate::error::CliError;
use crate::output::{show, Mode, Outcome};
use crate::shell::Shell;

pub async fn gc(shell: &Shell, dry_run: bool, look: &Look) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let collect = match dry_run {
        true => Collect::DryRun,
        false => Collect::Pass,
    };
    // The pass answers with a page a person reads, so what it says on the
    // way is said to a person too.
    let say = |remark: Remark| crate::remarks::say(remark, Mode::Human, look);
    let collected = project.collect(collect, &say, &shell.stop()).await?;
    show(look, &collected);
    Ok(Outcome::Success)
}
