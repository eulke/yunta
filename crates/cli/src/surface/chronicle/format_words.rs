//! The words a session's formatting is said in: what the project's command
//! formatted of what the session wrote, and whether the bound stopped it.

use std::path::PathBuf;

use yunta_core::CommandName;

/// What a session wrote, formatted by the project's command `run`.
pub(super) fn formatted(
    run: &CommandName,
    changed: &[PathBuf],
    code: i32,
    timed_out: bool,
) -> String {
    match (timed_out, code, changed) {
        (true, _, _) => format!("`{run}` was stopped while formatting what it wrote"),
        (false, 0, []) => format!("`{run}` left what it wrote as it was"),
        (false, 0, changed) => {
            let names: Vec<String> = changed
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            format!("`{run}` formatted {}", names.join(", "))
        }
        (false, code, _) => format!("`{run}` could not format what it wrote (exit {code})"),
    }
}
