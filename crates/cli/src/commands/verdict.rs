//! Whether a workflow can run here: one verdict, reached the same way for
//! every command that asks. `check` prints it, `new` prints it under the
//! file it wrote, `graph` prints it beside the drawing, and `run` refuses
//! on it. A workflow one of them accepted and another rejected would
//! leave a reader to work out which of the two to believe.

use std::path::{Path, PathBuf};

use surco::components::{Fields, Furrow, Headline, Next, Problem};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::look::Look;
use surco::text::counted;
use yunta_core::caution::Caution;
use yunta_core::step::NextStep;
use yunta_core::yaml::SourceMap;
use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::CheckError;
use yunta_render::blocks::diagnostic::located;

use super::advice;
use crate::context::Context;
use crate::error::{page, warn, CliError, Outcome};
use crate::output::{document, on_stderr};

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
    warnings: Vec<Caution>,
    /// The file the workflow was read from, which its refusals quote.
    path: PathBuf,
    /// What the file itself breaks, when the workflow judged is the one it
    /// declares once its unread keys are taken out.
    in_file: Vec<yunta_core::diagnostic::Diagnostic>,
}

/// What `workflow`, read from `path`, gets here under `config`.
/// `conflicts` are what the config's own layers refuse, which only a
/// caller that read the layers one by one knows. A stop while the files
/// the workflow reads are checked is the command's stop, not a verdict.
pub(crate) async fn verdict(
    ctx: &Context,
    workflow: &Workflow,
    path: &Path,
    config: &ConfigLayer,
    conflicts: Vec<String>,
    reach: Reach,
) -> Result<Verdict, CliError> {
    let facts = super::declared_facts(config);
    let mut errors = yunta_engine::check(workflow, config, &facts);
    let mut warnings = yunta_engine::check_warnings(workflow, config, &facts);
    // Composition references (`use:`) resolve against the repo catalog
    // under the current directory, then packs — the same catalog a run's
    // children resolve against at birth.
    let origin = yunta_engine::origin_of(&ctx.cwd, path);
    let refs = yunta_engine::check_workflow_refs(workflow, config, &ctx.cwd, &origin, &facts);
    errors.extend(refs.errors);
    warnings.extend(refs.warnings);
    let (unprovided, missing_programs) =
        super::refusals::environment(&ctx.env, &ctx.cwd, workflow, config, &origin);
    errors.extend(unprovided);
    warnings.extend(missing_programs);
    if reach == Reach::Workflow {
        let files =
            super::context_files_at_head(ctx, workflow, config.resolved_isolation()).await?;
        errors.extend(files.errors);
        warnings.extend(files.warnings);
    }
    Ok(Verdict {
        conflicts,
        errors,
        warnings,
        path: path.to_path_buf(),
        in_file: Vec::new(),
    })
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
    .await?;
    verdict.warn();
    if !verdict.passes() {
        return Err(verdict.refusal(ctx).await?);
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

    /// Whether the verdict has a warning to say.
    pub(crate) fn warns(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Says what does not stop the workflow but should be heard before it
    /// runs.
    pub(crate) fn warn(&self) {
        for warning in &self.warnings {
            warn(yunta_render::caution::line(warning));
        }
    }

    /// The verdict as a command that reports it draws it: its page on
    /// stdout when the workflow can run here, and on stderr, each problem
    /// pointed at in its line, when it cannot.
    pub(crate) async fn report(
        &self,
        ctx: &Context,
        path: &Path,
        look: &Look,
    ) -> Result<Outcome, CliError> {
        let page = self.page(ctx, path).await?;
        if self.passes() {
            document(look, page);
            return Ok(Outcome::Success);
        }
        on_stderr(look, page);
        Ok(Outcome::Reported)
    }

    /// The verdict as its page: whether the workflow read from `path` can
    /// run here, what it found, and the check to run again.
    pub(crate) async fn page(&self, ctx: &Context, path: &Path) -> Result<Doc, CliError> {
        let found = self.found(ctx).await?;
        let mark = match found.errors {
            0 => Mark::Done,
            _ => Mark::Failed,
        };
        let mut headline = Headline::new(mark, file_named(path), found.said());
        let Found { blocks, errors } = found;
        if !self.warnings.is_empty() {
            headline = headline.fact(counted(self.warnings.len(), "warning"));
        }
        let steps = match errors {
            0 => Vec::new(),
            _ => vec![check_again(&shown(ctx, path))],
        };
        Ok(Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with(Fields::new().push_if("at", shown(ctx, path)))
            .with_all(blocks)
            .with(Next { steps }))
    }

    /// What the verdict found, as the blocks a page draws: each problem in
    /// the order the file has them, each warning, and what this project
    /// could declare; and how many of them stop the workflow.
    pub(crate) async fn found(&self, ctx: &Context) -> Result<Found, CliError> {
        let unrunnable = self.unrunnable(ctx).await?;
        let warnings = self
            .warnings
            .iter()
            .map(|warning| Problem::warning(yunta_render::caution::line(warning)))
            .collect();
        Ok(Found {
            blocks: unrunnable.blocks(warnings),
            errors: unrunnable.problems.len(),
        })
    }

    /// The verdict as a run is refused with it. A stop while the project's
    /// remote is read is the outer `Err`, not a refusal.
    pub(crate) async fn refusal(&self, ctx: &Context) -> Result<CliError, CliError> {
        Ok(CliError::Unrunnable(self.unrunnable(ctx).await?))
    }

    /// Every problem, each about the workflow quoted from its file in the
    /// order the file has them, then what has no place in it, and what this
    /// project could declare for the ones a declaration fixes.
    async fn unrunnable(&self, ctx: &Context) -> Result<Unrunnable, CliError> {
        let text = tokio::fs::read_to_string(&self.path).await.ok();
        let map = text.as_deref().map(SourceMap::read).unwrap_or_default();
        let shown = shown(ctx, &self.path);
        let mut pointed: Vec<page::Pointed> = self
            .in_file
            .iter()
            .map(|diagnostic| page::found(diagnostic, &shown, text.as_deref()))
            .chain(self.errors.iter().map(|error| {
                let at = error.pointer().and_then(|pointer| map.place(&pointer));
                page::refused(error, at, &shown, text.as_deref()).into()
            }))
            .collect();
        // In the order the file has them; what has no place in it, a
        // config layer's conflict among them, after.
        pointed.sort_by_key(page::Pointed::order);
        let mut problems = page::hinted(pointed);
        problems.extend(
            self.conflicts
                .iter()
                .map(|conflict| located(conflict.clone(), None, &shown, None)),
        );
        let declare = match self.errors.is_empty() {
            true => Vec::new(),
            false => {
                let detected = crate::detect::Detected::in_repo(&ctx.cwd, ctx.supervision())
                    .await?
                    .for_errors(&self.errors)
                    .await;
                crate::detect::suggestions(&self.errors, &detected)
            }
        };
        Ok(Unrunnable { problems, declare })
    }
}

/// Why a workflow cannot run here: every problem, each pointed at in its
/// line when it has one, and what this project could declare for the ones
/// a declaration fixes.
#[derive(Debug)]
pub(crate) struct Unrunnable {
    pub(crate) problems: Vec<Problem>,
    pub(crate) declare: Vec<surco::components::Fix>,
}

impl Unrunnable {
    /// The blocks a page draws: each problem, then `warnings`, then what to
    /// declare.
    pub(crate) fn blocks(&self, warnings: Vec<Problem>) -> Vec<Block> {
        self.problems
            .iter()
            .cloned()
            .chain(warnings)
            .map(Block::from)
            .chain(self.declare.iter().flat_map(surco::components::Fix::blocks))
            .collect()
    }
}

/// What a verdict found, as a page draws it, and how many of the problems
/// stop the workflow.
pub(crate) struct Found {
    pub(crate) blocks: Vec<Block>,
    pub(crate) errors: usize,
}

impl Found {
    /// The verdict in words: the workflow can run here, or how many
    /// problems stop it.
    pub(crate) fn said(&self) -> String {
        match self.errors {
            0 => "can run here".to_string(),
            errors => counted(errors, "error"),
        }
    }
}

/// A workflow file as a headline names it: its name, the folder it is in
/// being what the lines its problems point at say.
pub(crate) fn file_named(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The check to run once what a verdict found in the file `shown` names
/// is fixed.
pub(crate) fn check_again(shown: &str) -> (String, &'static str) {
    (
        advice::spell(&NextStep::Check {
            workflow: shown.to_string(),
        }),
        "again, once they are fixed",
    )
}

/// `path` as a reader in this directory names it.
fn shown(ctx: &Context, path: &Path) -> String {
    surco::paths::shown(path, &ctx.cwd, ctx.env.home.as_deref())
}
