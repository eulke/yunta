//! See [`crate`]. A workflow file an interface names: the file a reference
//! resolves to, the workflow it declares read through the one door that holds
//! it to its own rules, or every problem it has read in one pass. A path
//! written relative is the workspace's, read from its directory and shown as
//! it was written.

use std::path::{Path, PathBuf};

use yunta_core::diagnostic::Report;
use yunta_core::Workflow;
use yunta_render::refusal::Refused;

use crate::{Error, Failure, Workspace};

/// A workflow file read: where it is, and the workflow it declares.
#[derive(Clone)]
pub struct WorkflowFile {
    pub path: PathBuf,
    pub workflow: Workflow,
}

/// A workflow file read in one pass, as `check` judges it: where it is, the
/// workflow it declares once the keys nothing reads are taken out, and every
/// problem the file has, so a person fixes them all in one round.
///
/// Not a [`WorkflowFile`], which a file breaking any rule never becomes.
#[derive(Clone)]
pub struct AuditedWorkflow {
    pub path: PathBuf,
    pub workflow: Workflow,
    pub problems: Report,
}

impl Workspace {
    /// The file a workflow reference names, the one rule every interface
    /// shares: a bare catalog name (no extension) resolves through the
    /// repository's catalog, then a publisher's vendored packs
    /// (`acme/review`); anything carrying an extension is taken as a literal
    /// path.
    pub fn workflow_file(&self, reference: &Path) -> Result<PathBuf, Error> {
        if reference.extension().is_some() {
            return Ok(reference.to_path_buf());
        }
        let global_root = yunta_core::user_state_root(&self.env)
            .unwrap_or_else(|| self.dir.clone())
            .join(".yunta/packs");
        let resolved =
            yunta_engine::resolve_workflow(&self.dir, &global_root, &reference.to_string_lossy())
                .map_err(Failure::new)?;
        Ok(resolved.path)
    }

    /// The workflow `reference` names, read through the one door that holds
    /// it to its own rules: a file that breaks one is refused, every problem
    /// pointed at in its text.
    pub fn read_workflow(&self, reference: &Path) -> Result<WorkflowFile, Error> {
        let path = self.workflow_file(reference)?;
        let workflow = read(&self.dir.join(&path), &path)?;
        Ok(WorkflowFile { path, workflow })
    }

    /// The workflow `reference` names, read in one pass: every problem the
    /// file has, beside the workflow it declares once the keys nothing reads
    /// are taken out. Refused, every problem pointed at in its text, when what
    /// is left does not read.
    pub fn audit_workflow(&self, reference: &Path) -> Result<AuditedWorkflow, Error> {
        let path = self.workflow_file(reference)?;
        let text = text_of(&self.dir.join(&path), &path)?;
        let (workflow, problems) = yunta_core::workflow::read::read_all(&text, &path);
        let Some(workflow) = workflow else {
            return Err(Refused::Document {
                report: Box::new(problems),
                text: Some(text),
            }
            .into());
        };
        Ok(AuditedWorkflow {
            path,
            workflow,
            problems,
        })
    }
}

/// The workflow in the file at `at`, read through the one door that holds it
/// to its own rules and named by `path`: a file that breaks them is refused,
/// quoted.
pub(crate) fn read(at: &Path, path: &Path) -> Result<Workflow, Error> {
    let text = text_of(at, path)?;
    yunta_core::workflow::read::read(&text, path).map_err(|report| {
        Refused::Document {
            report: Box::new(report),
            text: Some(text.clone()),
        }
        .into()
    })
}

/// The text of the workflow file at `at`, named by `path` when it does not
/// read.
fn text_of(at: &Path, path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(at).map_err(|source| {
        Failure::new(WorkflowUnread {
            path: path.to_path_buf(),
            source,
        })
        .into()
    })
}

/// A workflow file that could not be read off the disk.
#[derive(Debug, thiserror::Error)]
#[error("failed to read workflow at {}: {source}", path.display())]
struct WorkflowUnread {
    path: PathBuf,
    source: std::io::Error,
}

#[cfg(test)]
mod tests;
