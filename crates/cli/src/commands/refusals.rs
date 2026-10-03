//! What the machine a workflow would run on has to provide, read the
//! same way by every command that reaches a verdict on it.

use std::path::Path;

use yunta_core::{ConfigLayer, Workflow};

/// What the machine a run would use has to provide for `workflow`: what
/// the pack it comes from `requires:`, refused when this project or
/// machine lacks it, and the programs its literal commands start, warned
/// about when they are not on `PATH` — a node before them may install
/// them.
pub(crate) fn environment(
    cwd: &Path,
    workflow: &Workflow,
    config: &ConfigLayer,
    origin: &yunta_engine::WorkflowOrigin,
) -> (
    Vec<yunta_engine::CheckError>,
    Vec<yunta_engine::CheckWarning>,
) {
    let unprovided = match origin {
        yunta_engine::WorkflowOrigin::Pack {
            publisher,
            pack_name,
        } => yunta_engine::packs_for_publisher(cwd, publisher)
            .installed
            .iter()
            .find(|(_, manifest)| manifest.name == *pack_name)
            .map(|(_, manifest)| {
                yunta_engine::check_pack_requires(manifest, config).unmet(&command_on_path)
            })
            .unwrap_or_default(),
        yunta_engine::WorkflowOrigin::Repo => Vec::new(),
    };
    let missing_programs = yunta_engine::programs_named(workflow, config)
        .into_iter()
        .filter(|(_, program)| !command_on_path(program))
        .map(|(node, program)| yunta_engine::CheckWarning::ProgramNotOnPath { node, program })
        .collect();
    (unprovided, missing_programs)
}

/// Whether `command` names a file in a directory of this process's
/// `PATH` — the one a run started here inherits.
pub(crate) fn command_on_path(command: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(command).is_file())
}
