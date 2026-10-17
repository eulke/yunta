//! What the machine a workflow would run on has to provide, read the
//! same way by every command that reaches a verdict on it.

use std::path::Path;

use yunta_core::caution::Caution;
use yunta_core::{ConfigLayer, Env, Workflow};

/// What the machine a run would use has to provide for `workflow`: what
/// the pack it comes from `requires:`, refused when this project or
/// machine lacks it, and the programs its literal commands start, warned
/// about when they are not on `PATH` — a node before them may install
/// them.
pub(crate) fn environment(
    env: &Env,
    cwd: &Path,
    workflow: &Workflow,
    config: &ConfigLayer,
    origin: &yunta_engine::WorkflowOrigin,
) -> (Vec<yunta_engine::CheckError>, Vec<Caution>) {
    let on_path = |program: &str| command_on_path(env, program);
    let unprovided = match origin {
        yunta_engine::WorkflowOrigin::Pack {
            publisher,
            pack_name,
        } => yunta_engine::packs_for_publisher(cwd, publisher)
            .installed
            .iter()
            .find(|(_, manifest)| manifest.name == *pack_name)
            .map(|(_, manifest)| {
                yunta_engine::check_pack_requires(manifest, config).unmet(&on_path)
            })
            .unwrap_or_default(),
        yunta_engine::WorkflowOrigin::Repo => Vec::new(),
    };
    let missing_programs = yunta_engine::programs_named(workflow, config)
        .into_iter()
        .filter(|(_, program)| !command_on_path(env, program))
        .map(|(node, program)| Caution::ProgramNotOnPath { node, program })
        .collect();
    (unprovided, missing_programs)
}

/// Whether `command` names a file in a directory of `env`'s `PATH` — the
/// one a run started here inherits.
pub(crate) fn command_on_path(env: &Env, command: &str) -> bool {
    env.path.iter().any(|dir| dir.join(command).is_file())
}
