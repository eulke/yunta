//! See [`super`]. A new workflow, written as `.yunta/workflows/<name>.yaml`
//! from a minimal, commented skeleton: closer to `cargo new` than to a
//! working workflow, ruled paper to edit, not a working pipeline. It never
//! references a pack and never touches `yunta.lock`: a new workflow is the
//! team's own content, a pack someone else's, and the two stay disjoint on
//! purpose. What was written is judged as `check` judges a workflow, so a
//! skeleton the project's config cannot run is written, and said not to run.

use std::path::PathBuf;

use yunta_core::refusal::SetupRefusal;
use yunta_core::{CommandName, ConfigLayer, RunnerName, Workflow, WorkflowShape};
use yunta_render::page::Diagnosis;
use yunta_render::refusal::Refused;

use super::SetupUnwritten;
use crate::{Error, Failure, Judged, Stop, VerdictReach, Workspace};

/// The skeleton of `shape` named `stem`, as far as the project's `config`
/// declares what it names: a `lint` command, which a lint-fix skeleton then runs
/// by name instead of leaving a placeholder, and each runner a node names,
/// written uncommented where the config declares it.
fn skeleton(shape: WorkflowShape, stem: &WorkflowStem, config: &ConfigLayer) -> String {
    let template = match shape {
        WorkflowShape::OneNode => ONE_NODE_TEMPLATE,
        WorkflowShape::LintFix => LINT_FIX_TEMPLATE,
        WorkflowShape::Tasks => TASKS_TEMPLATE,
    };
    let lint = match config.command(&CommandName::from_static("lint")) {
        Some(_) => LINT_DECLARED,
        None => LINT_PLACEHOLDER,
    };
    let planner = RunnerName::from_static("planner");
    template
        .replace("{{workflow-name}}", stem.as_str())
        .replace("{{lint}}", lint)
        .replace(
            "{{implementer}}",
            &runner_line(&RunnerName::PROPOSED, config),
        )
        .replace("{{planner}}", &runner_line(&planner, config))
}

/// The `runner:` line a skeleton's node names `runner` with: uncommented where the
/// project's `config` declares it, and commented, to uncomment once it does,
/// where it does not.
fn runner_line(runner: &RunnerName, config: &ConfigLayer) -> String {
    let declared = config.runners.as_ref().is_some_and(|runners| {
        runners
            .get(runner)
            .is_some_and(|candidates| !candidates.is_empty())
    });
    match declared {
        true => format!("runner: {runner}  # as the project's config declares it"),
        false => format!("# runner: {runner}  # uncomment once runners: defines this role"),
    }
}

/// The stem a new workflow's file takes: letters, digits, `-` and `_` only,
/// and not empty, kept narrow so `<stem>.yaml` can never leave
/// `.yunta/workflows/`.
///
/// Not core's `WorkflowName`, the name a workflow declares inside its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStem(String);

impl WorkflowStem {
    /// `name` as a stem, or refused saying what a stem may hold.
    pub fn parse(name: &str) -> Result<Self, Error> {
        let refused = |refusal| Err(Refused::Setup(refusal).into());
        if name.is_empty() {
            return refused(SetupRefusal::EmptyName);
        }
        let safe = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        if !name.chars().all(safe) {
            return refused(SetupRefusal::UnsafeName {
                name: name.to_string(),
            });
        }
        Ok(WorkflowStem(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Workspace {
    /// Writes the workflow `stem` of `shape` as
    /// `.yunta/workflows/<stem>.yaml` from its skeleton, run by the project's
    /// own `lint` command where its config declares one. The skeleton is
    /// parsed before it is written, so `force` can never leave an invalid
    /// file behind. Refused when the file is there already and `force` does
    /// not say to write it again. An empty config, before `init`, is a legal
    /// one.
    pub async fn new_workflow(
        &self,
        stem: &WorkflowStem,
        shape: WorkflowShape,
        force: bool,
    ) -> Result<WrittenWorkflow<'_>, Error> {
        let layers = crate::config_layers(&self.dir, &self.env)?;
        let config = ConfigLayer::merge_layers(layers.into_iter().map(|(_, layer)| layer));
        let yaml = skeleton(shape, stem, &config);
        let workflow = Workflow::parse(&yaml).map_err(|error| {
            Failure::new(SkeletonUnparsed {
                shape: shape.label(),
                said: error.to_string(),
            })
        })?;
        let path = self
            .dir
            .join(".yunta/workflows")
            .join(format!("{}.yaml", stem.as_str()));
        if path.exists() && !force {
            return Err(Refused::Setup(SetupRefusal::Exists { path }).into());
        }
        write_skeleton(&path, &yaml).await?;
        Ok(WrittenWorkflow {
            workspace: self,
            path,
            workflow,
            config,
        })
    }
}

/// A workflow `new` wrote: where, and what it reads as, with the config it
/// was written against.
pub struct WrittenWorkflow<'w> {
    workspace: &'w Workspace,
    pub path: PathBuf,
    workflow: Workflow,
    config: ConfigLayer,
}

impl WrittenWorkflow<'_> {
    /// The verdict `check` reaches on the written workflow, under the
    /// project it was written in.
    pub async fn diagnosis(&self, stop: &Stop) -> Result<Diagnosis, Error> {
        let project = self.workspace.project_at(&self.workspace.dir)?;
        let judged = Judged {
            workflow: &self.workflow,
            path: &self.path,
            config: &self.config,
            conflicts: project.layer_conflicts()?,
        };
        let verdict = project
            .verdict(judged, VerdictReach::Workflow, stop)
            .await?;
        verdict.diagnosis(stop).await
    }
}

/// Writes `yaml` at `path`, its directory made first.
async fn write_skeleton(path: &std::path::Path, yaml: &str) -> Result<(), Error> {
    let unwritten =
        |doing: String, source| -> Error { Failure::new(SetupUnwritten { doing, source }).into() };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| unwritten(format!("create {}", parent.display()), source))?;
    }
    tokio::fs::write(path, yaml)
        .await
        .map_err(|source| unwritten(format!("write {}", path.display()), source))
}

/// A shape's skeleton that does not parse as a workflow: a bug in this
/// binary, caught before anything is written.
#[derive(Debug, thiserror::Error)]
#[error("the {shape} skeleton does not parse as a workflow: {said}")]
struct SkeletonUnparsed {
    shape: &'static str,
    said: String,
}

// Written as plain literals (not `format!`) so `{{`/`}}` YAML flow-mapping
// syntax and `{{run.dir}}` template braces stay literal — no escaping
// gymnastics to get wrong. `{{workflow-name}}`, `{{lint}}`,
// `{{implementer}}` and `{{planner}}` are the placeholders `skeleton`
// substitutes; none can collide with real workflow syntax (no schema key or
// template looks like one).

const ONE_NODE_TEMPLATE: &str = "\
name: {{workflow-name}}
# A minimal workflow: one node, one command. Its exit code *is* the
# verification: replace `run:` with something that actually checks
# your work, and narrow `scope:` to what this node may touch.
nodes:
  - id: main
    kind: bash
    run: \"true\"
    scope: [\"**\"]
";

const LINT_FIX_TEMPLATE: &str = "\
name: {{workflow-name}}
# A verify-then-correct chain: `lint` runs a check; if it fails,
# `on_failure.goto` reroutes to `fix`, which gets an
# agent session to address what's wrong, then control returns to
# `lint` to re-verify. `max_reroutes` caps how many correction
# rounds run before escalating to a person: one, since a round that
# did not fix it has told you what a person needs to look at.
nodes:
  - id: lint
    kind: bash
    {{lint}}
    on_failure: { goto: fix, max_reroutes: 1 }
  - id: fix
    kind: prompt
    {{implementer}}
    prompt: \"Fix what `lint` reported.\"
";

/// The lint step when the project declares its `lint` command: run by
/// the name `.yunta/config.yaml` gives it.
const LINT_DECLARED: &str =
    "run: { command: lint }  # the project's own lint command, as its config declares it";

/// The lint step when the project declares none.
const LINT_PLACEHOLDER: &str = "run: \"true\"  # replace with your real lint/test command";

const TASKS_TEMPLATE: &str = "\
name: {{workflow-name}}
# A tasks cycle, empty to start: `plan` opens an agent session that
# declares a tasks document and hands it over through the run tool the
# engine mounts for that kind; `implement` loops over it, dispatching
# one mechanically-verified session per ready task, until every task is
# `done`.
nodes:
  - id: plan
    kind: prompt
    {{planner}}
    prompt: \"Plan the work and hand over the tasks document.\"
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    {{implementer}}
    depends_on: [plan]
    until: all_tasks_complete
    prompt: \"Read your next task from the tasks document and implement it.\"
";
