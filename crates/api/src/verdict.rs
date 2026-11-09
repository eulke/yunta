//! See [`crate`]. Whether a workflow can run here: one verdict, reached the
//! same way for every interface that asks. `check` says it, `new` says it
//! under the file it wrote, `graph` says it beside the drawing, and a run is
//! refused on it. A workflow one of them accepted and another rejected would
//! leave a reader to work out which of the two to believe.
//!
//! What a verdict judges beyond the workflow's own check: the composition
//! references it resolves against the catalog, what the machine a run would
//! use has to provide (what the pack it comes from `requires:`, and the
//! programs its literal commands start), and the `files:` its nodes read in
//! the commit a run would start from.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use surco::components::{Fix, Problem};
use yunta_core::caution::Caution;
use yunta_core::check::{CheckError, Composition};
use yunta_core::diagnostic::{Diagnostic, Report};
use yunta_core::yaml::SourceMap;
use yunta_core::{CommitSha, ConfigLayer, Detected, Isolation, NodeId, Workflow};
use yunta_render::blocks::diagnostic::{hinted, located, Pointed};
use yunta_render::page::{Checked, Diagnosis, Unrunnable};
use yunta_render::refusal::Refused;

use crate::{Error, Failure, Project, Stop, Workspace};

/// How much of the project a verdict reads beyond the workflow itself.
///
/// Not the field's reach, which says which runs a listing is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictReach {
    /// Every node, and the files they read against the commit a run started
    /// now would start from: what `check`, `new` and `graph` ask about, since
    /// nothing has chosen a mode.
    Workflow,
    /// What a run asks before it is created. The files its nodes read are
    /// judged once its mode is known, against the commit it froze, so they
    /// are left to that moment rather than judged twice.
    RunFront,
}

/// A workflow as a verdict judges it: what it declares, the file it was read
/// from, and the config it would run under, with what that config's own layers
/// refuse among themselves.
pub struct Judged<'a> {
    pub workflow: &'a Workflow,
    pub path: &'a Path,
    pub config: &'a ConfigLayer,
    /// What the config's layers refuse among themselves, which only a caller
    /// that read the layers one by one knows: none for one merged file.
    pub conflicts: Vec<String>,
}

/// Everything that stops a workflow from running here, and everything a
/// reader should hear about it that does not.
pub struct Verdict<'p> {
    project: &'p Project,
    conflicts: Vec<String>,
    errors: Vec<CheckError>,
    cautions: Vec<Caution>,
    /// The file the workflow was read from, which its refusals quote.
    path: PathBuf,
    /// What the file itself breaks, when the workflow judged is the one it
    /// declares once its unread keys are taken out.
    in_file: Vec<Diagnostic>,
}

/// The literal `files:` paths the nodes of a workflow read that a run would
/// not find in the tree it starts from: each refused when nothing that runs
/// before its reader can write it, each a caution when something might.
#[derive(Debug, Default)]
pub struct ContextFiles {
    pub errors: Vec<CheckError>,
    pub cautions: Vec<Caution>,
}

impl Project {
    /// The verdict on `judged` here, as deep as `reach` asks. A stop while the
    /// files the workflow reads are checked is the caller's stop, not a
    /// verdict.
    pub async fn verdict(
        &self,
        judged: Judged<'_>,
        reach: VerdictReach,
        stop: &Stop,
    ) -> Result<Verdict<'_>, Error> {
        let Judged {
            workflow,
            path,
            config,
            conflicts,
        } = judged;
        let dir = &self.workspace.dir;
        let facts = crate::machine::declared_facts(config);
        let mut errors = yunta_engine::check(workflow, config, &facts);
        let mut cautions = yunta_engine::check_warnings(workflow, config, &facts);
        // Composition references (`use:`) resolve against the repo catalog
        // under the project's directory, then packs: the same catalog a run's
        // children resolve against at birth.
        let origin = yunta_engine::origin_of(dir, path);
        let refs = yunta_engine::check_workflow_refs(workflow, config, dir, &origin, &facts);
        errors.extend(refs.errors);
        cautions.extend(refs.warnings);
        let (unprovided, missing_programs) = self.environment(workflow, config, &origin);
        errors.extend(unprovided);
        cautions.extend(missing_programs);
        if reach == VerdictReach::Workflow {
            let isolation = config.resolved_isolation();
            let files = self
                .context_files_at_head(workflow, isolation, stop)
                .await?;
            errors.extend(files.errors);
            cautions.extend(files.cautions);
        }
        Ok(Verdict {
            project: self,
            conflicts,
            errors,
            cautions,
            path: path.to_path_buf(),
            in_file: Vec::new(),
        })
    }

    /// What this project's config layers refuse among themselves: a lower
    /// layer re-permitting what a higher one denied, naming both.
    pub fn layer_conflicts(&self) -> Result<Vec<String>, Error> {
        let layers = crate::config_layers(&self.workspace.dir, &self.workspace.env)?;
        let named: Vec<(&str, &ConfigLayer)> =
            layers.iter().map(|(name, layer)| (*name, layer)).collect();
        Ok(yunta_core::permission_layer_conflicts(&named))
    }

    /// The literal `files:` paths the nodes of `workflow` that `mode_nodes`
    /// includes (every node when `None`) read and a run started here from
    /// `base` would not find.
    pub async fn context_files(
        &self,
        workflow: &Workflow,
        mode_nodes: Option<&HashSet<NodeId>>,
        (isolation, base): (Isolation, &CommitSha),
        stop: &Stop,
    ) -> Result<ContextFiles, Error> {
        let origin = yunta_engine::RunTreeOrigin {
            checkout: &self.workspace.dir,
            isolation,
            base,
        };
        let supervision = self.workspace.supervision(stop);
        let found =
            yunta_engine::check_context_files(workflow, mode_nodes, origin, supervision).await?;
        Ok(ContextFiles {
            errors: found.errors,
            cautions: found.warnings,
        })
    }

    /// [`Project::context_files`] for a run started here now, from the
    /// commit the project's checkout is on. Every node counts, since nothing
    /// has chosen a mode. Nothing to say outside a repository with a commit,
    /// where no run starts either.
    pub async fn context_files_at_head(
        &self,
        workflow: &Workflow,
        isolation: Isolation,
        stop: &Stop,
    ) -> Result<ContextFiles, Error> {
        let supervision = self.workspace.supervision(stop);
        let base = match yunta_engine::head_commit(&self.workspace.dir, supervision).await {
            Ok(base) => base,
            Err(stopped) if stopped.cancelled() => return Err(Error::Stopped),
            Err(_) => return Ok(ContextFiles::default()),
        };
        self.context_files(workflow, None, (isolation, &base), stop)
            .await
    }

    /// What the machine a run would use has to provide for `workflow`: what
    /// the pack it comes from `requires:`, refused when this project or
    /// machine lacks it, and the programs its literal commands start, a
    /// caution when they are not on `PATH`, since a node before them may
    /// install them.
    fn environment(
        &self,
        workflow: &Workflow,
        config: &ConfigLayer,
        origin: &yunta_engine::WorkflowOrigin,
    ) -> (Vec<CheckError>, Vec<Caution>) {
        let env = &self.workspace.env;
        let on_path = |program: &str| env.finds(program);
        let unprovided = match origin {
            yunta_engine::WorkflowOrigin::Pack {
                publisher,
                pack_name,
            } => yunta_engine::packs_for_publisher(&self.workspace.dir, publisher)
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
            .filter(|(_, program)| !env.finds(program))
            .map(|(node, program)| Caution::ProgramNotOnPath { node, program })
            .collect();
        (unprovided, missing_programs)
    }
}

impl Workspace {
    /// `path` as a reader in this workspace's directory names it.
    pub fn shown(&self, path: &Path) -> String {
        surco::paths::shown(path, &self.dir, self.env.home.as_deref())
    }

    /// The file a workflow reference names, the one rule every interface
    /// shares: a bare catalog name (no extension) resolves through the
    /// repository's catalog, then a publisher's vendored packs
    /// (`acme/review`); anything carrying an extension is taken as a literal
    /// path.
    pub fn workflow_file(&self, reference: &Path) -> Result<PathBuf, Error> {
        if reference.extension().is_some() {
            return Ok(reference.to_path_buf());
        }
        let resolved = yunta_engine::resolve_workflow(&self.dir, &reference.to_string_lossy())
            .map_err(Failure::new)?;
        Ok(resolved.path)
    }
}

impl Verdict<'_> {
    /// Whether nothing stops the workflow.
    pub fn passes(&self) -> bool {
        self.conflicts.is_empty() && self.errors.is_empty() && self.in_file.is_empty()
    }

    /// This verdict with what the file itself breaks beside what the workflow
    /// it declares is refused for: one report, in the order the file has them.
    pub fn with_file_problems(mut self, in_file: Vec<Diagnostic>) -> Self {
        self.in_file = in_file;
        self
    }

    /// What does not stop the workflow but should be heard before it runs.
    pub fn cautions(&self) -> &[Caution] {
        &self.cautions
    }

    /// The verdict as its page says it, about the file it judged.
    pub async fn checked(&self, stop: &Stop) -> Result<Checked, Error> {
        Ok(Checked {
            path: self.path.clone(),
            at: self.project.workspace.shown(&self.path),
            diagnosis: self.diagnosis(stop).await?,
        })
    }

    /// What the verdict diagnosed: why the workflow cannot run here, each
    /// problem in the order the file has them, and each caution.
    pub async fn diagnosis(&self, stop: &Stop) -> Result<Diagnosis, Error> {
        let warnings = self
            .cautions
            .iter()
            .map(|caution| Problem::warning(yunta_render::caution::line(caution)))
            .collect();
        Ok(Diagnosis {
            unrunnable: self.unrunnable(stop).await?,
            warnings,
        })
    }

    /// The verdict as a run is refused with it.
    pub async fn refusal(&self, stop: &Stop) -> Result<Refused, Error> {
        Ok(Refused::Unrunnable(Box::new(self.unrunnable(stop).await?)))
    }

    /// Every problem, each about the workflow quoted from its file in the
    /// order the file has them, then what has no place in it, and what this
    /// project could declare for the ones a declaration fixes.
    async fn unrunnable(&self, stop: &Stop) -> Result<Unrunnable, Error> {
        let workspace = &self.project.workspace;
        let text = tokio::fs::read_to_string(&self.path).await.ok();
        let map = text.as_deref().map(SourceMap::read).unwrap_or_default();
        let shown = workspace.shown(&self.path);
        let mut pointed: Vec<Pointed> = self
            .in_file
            .iter()
            .map(|diagnostic| Pointed::found(diagnostic, &shown, text.as_deref()))
            .collect();
        let mut composed = Vec::new();
        for error in &self.errors {
            match error {
                CheckError::Composition(Composition::Unparseable { report, .. }) => {
                    composed.extend(composed_problems(workspace, report).await);
                }
                error => {
                    let at = error.pointer().and_then(|pointer| map.place(&pointer));
                    pointed.push(
                        yunta_render::refusal::check::problem(error, at, &shown, text.as_deref())
                            .into(),
                    );
                }
            }
        }
        // In the order the file has them; what has no place in it, a config
        // layer's conflict among them, after.
        pointed.sort_by_key(Pointed::order);
        // A file this one composes is another file: its problems follow, in
        // its own order.
        pointed.extend(composed);
        let mut problems = hinted(pointed);
        problems.extend(
            self.conflicts
                .iter()
                .map(|conflict| located(conflict.clone(), None, &shown, None)),
        );
        Ok(Unrunnable {
            workflow: shown,
            problems,
            declare: self.declarations(stop).await?,
        })
    }

    /// What this project could declare for the problems a declaration fixes,
    /// from what the repository and this machine answer for: none when
    /// nothing is refused.
    async fn declarations(&self, stop: &Stop) -> Result<Vec<Fix>, Error> {
        if self.errors.is_empty() {
            return Ok(Vec::new());
        }
        let workspace = &self.project.workspace;
        let in_repo = crate::machine::in_repo(workspace, &workspace.dir, stop).await?;
        let detected = Detected {
            adapters: crate::machine::adapters_for(&self.errors).await,
            ..in_repo
        };
        Ok(crate::machine::suggestions(&self.errors, &detected))
    }
}

/// Every problem of a workflow this one composes that does not read, each
/// quoted from its own file.
async fn composed_problems(workspace: &Workspace, report: &Report) -> Vec<Pointed> {
    let path = Path::new(&report.document.path);
    let text = tokio::fs::read_to_string(path).await.ok();
    let shown = workspace.shown(path);
    report
        .diagnostics
        .iter()
        .map(|diagnostic| Pointed::found(diagnostic, &shown, text.as_deref()))
        .collect()
}
