//! `yunta check <workflow>`: static validation of a workflow — no run
//! created, no agent involved. Resolves a bare catalog name the same way
//! `run` and `graph` do, judges it against the project's real config layers
//! (or a single explicit `--config` file), and reports every error the
//! engine's `check` finds, its warnings, and any verification-effectiveness
//! findings from this workflow's past runs. The check itself is the API's,
//! which the application shows too.

use std::path::Path;

use surco::look::Look;

use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{blank, show};
use crate::shell::Shell;

pub async fn check(
    shell: &Shell,
    workflow_path: &Path,
    config_path: Option<&Path>,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let check = project
        .check(workflow_path, config_path, &shell.stop())
        .await?;
    // The page on stdout when the workflow can run here, and on stderr, each
    // problem pointed at in its line, when it cannot.
    let outcome = match check.passes {
        true => {
            show(look, &check.checked);
            Outcome::Success
        }
        false => {
            crate::output::show_on_stderr(look, &check.checked);
            Outcome::Reported
        }
    };
    // Advice after the verdict, which is what was asked for.
    if !check.findings.is_empty() {
        blank();
        show(look, &check.findings);
    }
    Ok(outcome)
}
