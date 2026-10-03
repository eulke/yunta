//! Whether a workflow can run here: one verdict, reached the same way for
//! every command that asks. `check` prints it, `new` prints it under the
//! file it wrote, `graph` prints it beside the drawing, and `run` refuses
//! on it. A workflow one of them accepted and another rejected would
//! leave a reader to work out which of the two to believe.

use std::fmt::Display;
use std::path::{Path, PathBuf};

use yunta_core::yaml::{Location, SourceMap};
use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::CheckError;

use crate::context::Context;
use crate::error::{note, warn, CliError, Outcome};
use crate::render::blocks::diagnostic::located;

/// How much of the project a verdict reads beyond the workflow itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reach {
    /// Every node, and the files they read against the commit a run
    /// started now would start from: what `check`, `new` and `graph`
    /// ask about, since nothing has chosen a mode.
    Workflow,
    /// What a run asks before it is created. The files its nodes read
    /// are judged once its mode is known, against the commit it froze,
    /// so they are left to that moment rather than judged twice.
    RunFront,
}

/// Everything that stops a workflow from running here, and everything a
/// reader should hear about it that does not.
pub(crate) struct Verdict {
    /// Layers of config that try to loosen what a layer above them
    /// denies.
    conflicts: Vec<String>,
    errors: Vec<CheckError>,
    warnings: Vec<String>,
    /// The file the workflow was read from, which its refusals quote.
    path: PathBuf,
    /// What the file itself breaks, when the workflow judged is the one it
    /// declares once its unread keys are taken out.
    in_file: Vec<yunta_core::diagnostic::Diagnostic>,
}

/// What `workflow`, read from `path`, gets here under `config`.
/// `conflicts` are what the config's own layers refuse, which only a
/// caller that read the layers one by one knows.
pub(crate) async fn verdict(
    ctx: &Context,
    workflow: &Workflow,
    path: &Path,
    config: &ConfigLayer,
    conflicts: Vec<String>,
    reach: Reach,
) -> Verdict {
    let mut errors = yunta_engine::check(workflow, config, &super::declared_capabilities);
    let mut warnings: Vec<String> = yunta_engine::check_warnings(workflow, config)
        .iter()
        .map(ToString::to_string)
        .collect();
    // Composition references (`use:`) resolve against the repo catalog
    // under the current directory, then packs — the same catalog a run's
    // children resolve against at birth.
    let origin = yunta_engine::origin_of(&ctx.cwd, path);
    let refs = yunta_engine::check_workflow_refs(
        workflow,
        config,
        &ctx.cwd,
        &origin,
        &super::declared_capabilities,
    );
    errors.extend(refs.errors);
    warnings.extend(refs.warnings.iter().map(ToString::to_string));
    let (unprovided, missing_programs) =
        super::refusals::environment(&ctx.cwd, workflow, config, &origin);
    errors.extend(unprovided);
    warnings.extend(missing_programs.iter().map(ToString::to_string));
    if reach == Reach::Workflow {
        let files = super::context_files_at_head(ctx, workflow, config.resolved_isolation()).await;
        errors.extend(files.errors);
        warnings.extend(files.warnings.iter().map(ToString::to_string));
    }
    Verdict {
        conflicts,
        errors,
        warnings,
        path: path.to_path_buf(),
        in_file: Vec::new(),
    }
}

/// A workflow reference resolved to its file and loaded, refused if it
/// fails `yunta check` — the shared front of `yunta run` and the control
/// plane's `run_workflow`, so both reach the same workflow the same way: a
/// bare catalog name (no extension) resolves through the repo catalog then
/// a publisher's vendored packs (`acme/review`); anything with an
/// extension is taken as a literal path.
pub(crate) async fn resolve_runnable(
    ctx: &Context,
    workflow_path: &Path,
) -> Result<(PathBuf, Workflow), CliError> {
    let resolved = super::resolve_workflow_ref(&ctx.cwd, workflow_path)?;
    let workflow = crate::load_workflow(&resolved)?;
    let verdict = verdict(
        ctx,
        &workflow,
        &resolved,
        &ctx.project.config,
        layer_conflicts(&ctx.cwd)?,
        Reach::RunFront,
    )
    .await;
    verdict.warn();
    if !verdict.passes() {
        return Err(verdict.refusal(ctx).await);
    }
    Ok((resolved, workflow))
}

/// What the project's config layers refuse among themselves: a lower
/// layer re-permitting what a higher one denied, naming both.
pub(crate) fn layer_conflicts(cwd: &Path) -> Result<Vec<String>, CliError> {
    let layers = crate::project::load_named_layers(cwd)?;
    let named: Vec<(&str, &ConfigLayer)> =
        layers.iter().map(|(name, layer)| (*name, layer)).collect();
    Ok(yunta_core::permission_layer_conflicts(&named))
}

impl Verdict {
    /// Whether nothing stops the workflow.
    pub(crate) fn passes(&self) -> bool {
        self.conflicts.is_empty() && self.errors.is_empty() && self.in_file.is_empty()
    }

    /// This verdict with what the file itself breaks beside what the
    /// workflow it declares is refused for: one report, in the order the
    /// file has them.
    pub(crate) fn with_file_problems(
        mut self,
        in_file: Vec<yunta_core::diagnostic::Diagnostic>,
    ) -> Self {
        self.in_file = in_file;
        self
    }

    /// Says what does not stop the workflow but should be heard before it
    /// runs.
    pub(crate) fn warn(&self) {
        for warning in &self.warnings {
            warn(warning);
        }
    }

    /// The verdict as a command that reports it says it: the warnings,
    /// then `<subject>: OK` on stdout, or every problem on stderr with
    /// what this project could declare to fix it.
    pub(crate) async fn report(&self, ctx: &Context, subject: impl Display) -> Outcome {
        self.warn();
        if self.passes() {
            println!("{subject}: OK");
            return Outcome::Success;
        }
        note(self.problems(ctx, subject).await);
        Outcome::Reported
    }

    /// The verdict as a run is refused with it.
    pub(crate) async fn refusal(&self, ctx: &Context) -> CliError {
        CliError::msg(self.problems(ctx, "the workflow fails `yunta check`").await)
    }

    /// Every problem under `heading`, each refusal about the workflow
    /// quoted from its file, then what this project could declare for the
    /// ones a declaration fixes.
    async fn problems(&self, ctx: &Context, heading: impl Display) -> String {
        let text = tokio::fs::read_to_string(&self.path).await.ok();
        let map = text.as_deref().map(SourceMap::read).unwrap_or_default();
        let mut all: Vec<(String, Option<Location>)> = self
            .in_file
            .iter()
            .map(|problem| match problem.at {
                Some(at) => (problem.said(), Some(at)),
                None => (problem.to_string(), None),
            })
            .chain(self.errors.iter().map(|error| {
                let at = error.pointer().and_then(|pointer| map.place(&pointer));
                (error.to_string(), at)
            }))
            .collect();
        // In the order the file has them; what has no place in it — a
        // config layer's conflict among them — after.
        all.sort_by_key(|(_, at)| at.map_or((usize::MAX, 0), |at| (at.line, at.col)));
        all.extend(
            self.conflicts
                .iter()
                .map(|conflict| (conflict.clone(), None)),
        );
        let shown = self.path.display().to_string();
        let mut said = located(&heading.to_string(), &all, &shown, text.as_deref());
        let detected = crate::detect::Detected::in_repo(&ctx.cwd, ctx.supervision())
            .await
            .for_errors(&self.errors)
            .await;
        for line in crate::detect::suggestions(&self.errors, &detected) {
            said.push_str(&format!("\n  {line}"));
        }
        said
    }
}
