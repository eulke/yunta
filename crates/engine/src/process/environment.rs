//! What the engine's commands run with: the shell every one of them is
//! handed to, and the `PATH` that shell looks programs up in.

use std::ffi::OsStr;

use yunta_core::process::SearchPath;

/// The program every command the engine runs is handed to.
pub(super) const SHELL: &str = "sh";

/// What the engine's commands run with, as a run records it: the shell
/// found on the `PATH` they are looked up in, and that `PATH` — the
/// ambient one, or the one `subprocess_vars` sets. `None` without an
/// ambient environment to read, which only a harness runs without.
pub fn execution_environment(
    ambient: Option<&yunta_core::Env>,
) -> Option<yunta_core::events::ExecutionEnvironment> {
    let ambient = ambient?;
    let search_path = match ambient
        .subprocess_vars
        .iter()
        .rev()
        .find(|(name, _)| name == "PATH")
    {
        Some((_, value)) => SearchPath::of_var(OsStr::new(value)),
        None => ambient.path.clone(),
    };
    let shell = search_path
        .find(SHELL)
        .map_or_else(|| SHELL.to_string(), |found| found.display().to_string());
    Some(yunta_core::events::ExecutionEnvironment {
        shell,
        path: search_path
            .dirs()
            .iter()
            .map(|dir| dir.display().to_string())
            .collect(),
    })
}
