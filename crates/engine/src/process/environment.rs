//! What the engine's commands run with: the shell every one of them is
//! handed to, and the `PATH` that shell looks programs up in.

use std::path::PathBuf;

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
    let path: Vec<PathBuf> = match ambient
        .subprocess_vars
        .iter()
        .rev()
        .find(|(name, _)| name == "PATH")
    {
        Some((_, value)) => std::env::split_paths(value).collect(),
        None => ambient.path.clone(),
    };
    let shell = path
        .iter()
        .map(|dir| dir.join(SHELL))
        .find(|candidate| candidate.is_file())
        .map_or_else(|| SHELL.to_string(), |found| found.display().to_string());
    Some(yunta_core::events::ExecutionEnvironment {
        shell,
        path: path.iter().map(|dir| dir.display().to_string()).collect(),
    })
}
