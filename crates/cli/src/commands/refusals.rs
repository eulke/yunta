//! What a workflow is refused for before a run exists: every problem
//! `check` finds, and what the machine it would run on lacks.

use std::path::Path;

use yunta_core::{ConfigLayer, Workflow};

use super::declared_capabilities;
use crate::error::{warn, CliError};

/// `yunta check` before running anything — a workflow that fails static
/// validation never creates a run. `workflow_path` is where `workflow`
/// itself was loaded from — needed to tell `check_workflow_refs`
/// whether this workflow already lives inside a pack, since the
/// cross-pack composition rule only applies once you're inside one.
pub(crate) fn check_or_refuse(
    cwd: &Path,
    workflow: &Workflow,
    config: &ConfigLayer,
    workflow_path: &Path,
) -> Result<(), CliError> {
    // Warnings (e.g. a `parallel` group that can't verify its children
    // won't collide) are visible but never block — only `check()`'s
    // errors do.
    for warning in yunta_engine::check_warnings(workflow, config) {
        warn(warning);
    }
    let mut errors = yunta_engine::check(workflow, config, &declared_capabilities);
    // The composition reference graph (`use:` names resolve, acyclic,
    // within depth) reads the repo catalog under the current
    // directory — the same `.yunta/workflows/` a run's children resolve
    // against at birth.
    let origin = yunta_engine::origin_of(cwd, workflow_path);
    let refs =
        yunta_engine::check_workflow_refs(workflow, config, cwd, &origin, &declared_capabilities);
    for warning in &refs.warnings {
        warn(warning);
    }
    errors.extend(refs.errors);
    let (unprovided, missing_programs) = environment(cwd, workflow, config, &origin);
    for warning in &missing_programs {
        warn(warning);
    }
    errors.extend(unprovided);
    if errors.is_empty() {
        return Ok(());
    }
    Err(CliError::msg(yunta_core::text::problems(
        "the workflow fails `yunta check`",
        &errors,
    )))
}

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
