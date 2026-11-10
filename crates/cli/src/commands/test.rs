//! `yunta test`: runs a project's cases through the API, those under a
//! project root's `.yunta/tests/` (the current directory, or `--dir`), each
//! drawn as it ends in the one column every case's name fits, then the
//! tally. No LLM, no network: every session is scripted.

use std::path::{Path, PathBuf};

use surco::look::Look;
use yunta_api::Remark;
use yunta_render::page::TestedCase;

use crate::error::CliError;
use crate::output::{show, Mode, Outcome};
use crate::shell::Shell;

pub async fn test(shell: &Shell, dir: Option<&Path>, look: &Look) -> Result<Outcome, CliError> {
    let workspace = shell.workspace();
    let root = project_root(dir.unwrap_or(&workspace.dir)).map_err(CliError::msg)?;
    let suite = workspace.cases(&root)?;
    // Every case's name is known before the first one runs, so the verdicts
    // line up in one column as each case finishes.
    let names = suite.names();
    let shown = |case| {
        let row = TestedCase {
            names: &names,
            case,
        };
        show(look, &row);
    };
    let say = |remark: Remark| crate::remarks::say(remark, Mode::Human, look);
    // One interruption for the whole suite: a Ctrl-C stops the case that is
    // running, and the ones after it stop as they start.
    let tested = suite.run(shown, &say, &shell.stop()).await;
    crate::output::blank();
    show(look, &tested);
    Ok(match tested.failures {
        0 => Outcome::Success,
        _ => Outcome::Reported,
    })
}

/// The project whose `.yunta/` holds the cases: `dir`, the one given or the
/// shell's. Always absolute, so every case's sandbox and fixture resolve
/// from one fixed root wherever the command was invoked.
fn project_root(dir: &Path) -> Result<PathBuf, String> {
    let root = dir.to_path_buf();
    std::path::absolute(&root).map_err(|e| format!("cannot resolve `{}`: {e}", root.display()))
}
