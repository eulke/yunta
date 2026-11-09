//! See [`crate`]. A run admitted: everything it settles before it exists. Its
//! workflow is resolved and judged, git can commit its work, the forge it
//! opens a pull request through is reachable, the manifest is frozen with the
//! adapter its sessions are pinned to, the adapters it reaches are present and
//! healthy, its mode's own check passes, and what is said before the first
//! token is said. What this refuses costs nothing, and what it admits is the
//! very run the caller is about to create: every shape of a run, from every
//! interface, comes through here, so the same workflow is refused in the same
//! words wherever it is asked for.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use yunta_core::caution::Caution;
use yunta_core::step::Arg;
use yunta_core::{InputName, Manifest, ModeName, RunAdapter, Workflow};
use yunta_engine::FrozenRun;
use yunta_render::document::PreRunWarnings;
use yunta_render::refusal::Refused;
use yunta_replay::history::PriorEstimation;

use crate::{
    Committer, Error, Failure, Judged, Project, Remark, Remarks, Stop, Verdict, VerdictReach,
};

/// What a run is asked for with: its workflow by reference, its inputs by
/// name, the mode it starts in, and the adapter its sessions are pinned to.
pub struct RunRequest<'a> {
    /// A catalog name, or a path to the workflow's file.
    pub workflow: &'a Path,
    pub inputs: HashMap<InputName, String>,
    /// The mode it starts in: the workflow's first mode when none is named.
    pub mode: Option<&'a ModeName>,
    /// One adapter this binary builds, by its id, or a mock fixture's text:
    /// every session runs on it. None, and runner resolution picks.
    pub adapter: Option<RunAdapter>,
    /// Whether the run freezes none of the person's own knowledge, as a test
    /// case does: its scripted sessions read none, so nothing of the machine
    /// it runs on reaches it.
    pub hermetic: bool,
}

/// A run admitted and not yet created: its frozen manifest, the mode it
/// starts in, and what was said before the first token.
pub struct Admission<'p> {
    pub(crate) project: &'p Project,
    pub(crate) frozen: FrozenRun,
    pub(crate) mode: ModeName,
    /// What this workflow's past runs cost, for the comparison a run's
    /// closing draws: `None` below the history an estimate needs.
    pub prior: Option<PriorEstimation>,
    /// Every caution said before the first token, kept for the document a
    /// reader with no terminal reads.
    pub warnings: PreRunWarnings,
}

impl Project {
    /// The run `request` asks for, admitted, every caution and the estimate
    /// said through `remarks` in the order they are said; refused, at no
    /// cost, when anything stops it.
    pub async fn admit(
        &self,
        request: RunRequest<'_>,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<Admission<'_>, Error> {
        let (path, workflow) = self.runnable(request.workflow, remarks, stop).await?;
        self.refuse_without_committer(stop).await?;
        if !matches!(request.adapter, Some(RunAdapter::Mock { .. })) {
            self.refuse_unreachable_forge(&workflow)?;
        }
        let frozen = self
            .freeze(&workflow, &path, &request, stop)
            .await?
            .with_adapter(request.adapter);
        let mode = request
            .mode
            .cloned()
            .unwrap_or_else(|| frozen.manifest.workflow.first_mode());
        let shown = Arg::Given(self.workspace.shown(&path));
        crate::machine::servable(&frozen.manifest, &mode, shown).await?;
        let reference = Arg::Given(self.workspace.shown(request.workflow));
        let refused = |errors| Refused::Unchecked {
            workflow: reference.clone(),
            errors,
        };
        // A run started here is started fresh in its mode: what only a run
        // promoted into that mode would hold, it does not.
        let errors = yunta_engine::check_mode_start(&frozen.manifest.workflow, &mode);
        if !errors.is_empty() {
            return Err(refused(errors).into());
        }
        let context_files = self.context_files_of(&frozen.manifest, &mode, stop).await?;
        if !context_files.errors.is_empty() {
            return Err(refused(context_files.errors).into());
        }
        for caution in &context_files.cautions {
            remarks.remark(Remark::Caution(caution.clone()));
        }
        let (prior, budget) = self.estimate(&frozen.manifest, &mode, remarks).await;
        // The document lists the budget first, then the files a node reads.
        let cautions = budget.into_iter().chain(context_files.cautions).collect();
        Ok(Admission {
            project: self,
            frozen,
            mode,
            prior,
            warnings: PreRunWarnings { cautions },
        })
    }

    /// The workflow `reference` names, read and judged as a run asks before it
    /// is created: each caution said, and refused when the verdict does not
    /// pass.
    async fn runnable(
        &self,
        reference: &Path,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<(PathBuf, Workflow), Error> {
        let path = self.workspace.workflow_file(reference)?;
        let workflow = read_workflow(&path)?;
        let judged = Judged {
            workflow: &workflow,
            path: &path,
            config: &self.config,
            conflicts: self.layer_conflicts()?,
        };
        let verdict: Verdict<'_> = self.verdict(judged, VerdictReach::RunFront, stop).await?;
        for caution in verdict.cautions() {
            remarks.remark(Remark::Caution(caution.clone()));
        }
        if !verdict.passes() {
            return Err(verdict.refusal(stop).await?.into());
        }
        Ok((path, workflow))
    }

    /// Refuses a run in a repository git cannot commit in.
    async fn refuse_without_committer(&self, stop: &Stop) -> Result<(), Error> {
        match self.workspace.committer(stop).await? {
            Committer::Named(_) => Ok(()),
            Committer::Unnamed { git_said } => Err(Refused::NoCommitter { git_said }.into()),
        }
    }

    /// Refuses a run that would open a pull request through a forge this
    /// machine cannot reach: the config declares one, and the variable its
    /// token is in is not set here. The node would fail when the run reaches
    /// it, after every node before it spent; a `pull_request` node the run
    /// leaves out asks nothing.
    fn refuse_unreachable_forge(&self, workflow: &Workflow) -> Result<(), Error> {
        let config = &self.config;
        let Some(github) = config
            .forge
            .as_ref()
            .and_then(|forge| forge.github.as_ref())
        else {
            return Ok(());
        };
        let reaches = yunta_core::opens_a_pull_request(workflow, config);
        if !reaches || self.workspace.secrets.get(&github.token_env).is_some() {
            return Ok(());
        }
        Err(Refused::ForgeUnreachable {
            repo: github.repo.to_string(),
            token_env: github.token_env.clone(),
        }
        .into())
    }

    /// Freezes the run: its manifest, every input its nodes read, the
    /// documents its `inputs:` named, and its state roots, which a manifest
    /// holds absolute — `resume`, `status` and `gc` read them back from any
    /// directory, so a relative root is refused here, naming it.
    async fn freeze(
        &self,
        workflow: &Workflow,
        path: &Path,
        request: &RunRequest<'_>,
        stop: &Stop,
    ) -> Result<FrozenRun, Error> {
        let workflow_dir = path.parent().unwrap_or(Path::new("."));
        // The user layer's knowledge, where the environment keeps it.
        let user_knowledge = match request.hermetic {
            true => None,
            false => yunta_core::user_knowledge_dir(&self.workspace.env),
        };
        let mut frozen = yunta_engine::build_manifest(
            workflow,
            &self.config,
            workflow_dir,
            &self.workspace.dir,
            &request.inputs,
            self.workspace.supervision(stop),
            user_knowledge.as_deref(),
        )
        .await?;
        let paths =
            yunta_core::FrozenPaths::new(self.runs_root.clone(), self.worktrees_root.clone())
                .map_err(Failure::new)?;
        frozen.manifest.paths = Some(paths);
        Ok(frozen)
    }

    /// The literal `files:` paths the nodes of this run's mode read that the
    /// commit it starts from does not hold: said now, before the nodes ahead
    /// of them spend, which is when the run itself would find out.
    async fn context_files_of(
        &self,
        manifest: &Manifest,
        mode: &ModeName,
        stop: &Stop,
    ) -> Result<crate::ContextFiles, Error> {
        let left_out = yunta_core::left_out(&manifest.workflow, &manifest.config);
        let included = yunta_replay::modes::scheduled(&manifest.workflow, mode, &left_out);
        let at = (manifest.isolation, &manifest.base_commit);
        self.context_files(&manifest.workflow, included.as_ref(), at, stop)
            .await
    }

    /// What this workflow's past runs in `mode` cost, said before anything is
    /// spent, and the caution a token cap below it raises: a run that stops
    /// halfway on a badly chosen cap is the most expensive waste there is.
    async fn estimate(
        &self,
        manifest: &Manifest,
        mode: &ModeName,
        remarks: &dyn Remarks,
    ) -> (Option<PriorEstimation>, Option<Caution>) {
        let estimation = self.prior_estimation(manifest, mode, None).await;
        if let Some(estimation) = &estimation {
            remarks.remark(Remark::Estimate(estimation.clone()));
        }
        let cap = manifest
            .config
            .limits
            .as_ref()
            .and_then(|limits| limits.max_tokens_per_run);
        let budget = yunta_replay::history::budget_caution(cap, estimation.as_ref());
        if let Some(caution) = &budget {
            remarks.remark(Remark::Caution(caution.clone()));
        }
        (estimation, budget)
    }
}

impl Admission<'_> {
    /// The manifest the run is created with.
    pub fn manifest(&self) -> &Manifest {
        &self.frozen.manifest
    }

    /// The mode the run starts in.
    pub fn mode(&self) -> &ModeName {
        &self.mode
    }
}

/// A workflow file that could not be read off the disk.
#[derive(Debug, thiserror::Error)]
#[error("failed to read workflow at {}: {source}", path.display())]
struct WorkflowUnread {
    path: PathBuf,
    source: std::io::Error,
}

/// The workflow at `path`, read through the one door that holds it to its
/// own rules: a file that breaks them is refused, quoted.
fn read_workflow(path: &Path) -> Result<Workflow, Error> {
    let contents = std::fs::read_to_string(path).map_err(|source| {
        Failure::new(WorkflowUnread {
            path: path.to_path_buf(),
            source,
        })
    })?;
    yunta_core::workflow::read::read(&contents, path).map_err(|report| {
        Refused::Document {
            report: Box::new(report),
            text: Some(contents.clone()),
        }
        .into()
    })
}
