//! `yunta new <name> [--shape ...]`: writes
//! `.yunta/workflows/<name>.yaml` from a minimal, commented schema
//! skeleton — closer to `cargo new` than to a working workflow: ruled
//! paper to edit, not a working pipeline. Never references a pack and
//! never touches `yunta.lock` — `new` creates the team's own content,
//! `pack add` is the only verb that brings in someone else's, and the
//! two stay disjoint on purpose. Reaches the verdict `yunta check` would
//! on what it wrote and reports it the same way, exit code included: a
//! skeleton the project's config cannot run is written, and said not to
//! run.

use std::path::{Path, PathBuf};

use surco::components::{Fields, Furrow, Headline, Next};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::{ConfigLayer, Workflow};

use super::advice;
use super::verdict::{check_again, file_named, Found};
use crate::ask::{choose, Choice, Console, Escape, NoAnswer};
use crate::error::{CliError, Outcome};
use crate::output::document;
use crate::project;
use crate::surface::Diagnostics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    OneNode,
    LintFix,
    Tasks,
}

impl Shape {
    /// The shape `--shape` names, or a message listing the ones that
    /// exist. Parsed by [`Shape::label`], the same string the chooser
    /// prints and `new` reports, so a shape cannot be spelled one way
    /// to a reader and another to the parser.
    pub fn parse(name: &str) -> Result<Self, String> {
        Self::all()
            .into_iter()
            .find(|shape| shape.label() == name)
            .ok_or_else(|| format!("unknown shape `{name}`; choose one of: {}", Self::listed()))
    }

    fn all() -> [Self; 3] {
        [Self::OneNode, Self::LintFix, Self::Tasks]
    }

    /// The shapes as a sentence lists them — one home for "one of ...",
    /// so it cannot fall behind [`Shape::all`].
    fn listed() -> String {
        Self::all()
            .iter()
            .map(|shape| shape.label())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn label(&self) -> &'static str {
        match self {
            Self::OneNode => "one-node",
            Self::LintFix => "lint-fix",
            Self::Tasks => "tasks",
        }
    }

    /// What a workflow of this shape does, beside its name on the menu.
    fn does(&self) -> &'static str {
        match self {
            Self::OneNode => "one command whose exit code is the check",
            Self::LintFix => "a check, and an agent that fixes what it finds",
            Self::Tasks => "a plan of tasks, and an agent that works through them",
        }
    }

    /// The skeleton named `name`. `lint_declared` says whether the
    /// project's config declares a `lint` command, which a lint-fix
    /// skeleton then runs by name instead of leaving a placeholder.
    fn skeleton(&self, name: &str, lint_declared: bool) -> String {
        let template = match self {
            Self::OneNode => ONE_NODE_TEMPLATE,
            Self::LintFix => LINT_FIX_TEMPLATE,
            Self::Tasks => TASKS_TEMPLATE,
        };
        let lint = match lint_declared {
            true => LINT_DECLARED,
            false => LINT_PLACEHOLDER,
        };
        template
            .replace("{{workflow-name}}", name)
            .replace("{{lint}}", lint)
    }
}

// Written as plain literals (not `format!`) so `{{`/`}}` YAML flow-mapping
// syntax and `{{run.dir}}` template braces stay literal — no escaping
// gymnastics to get wrong. `{{workflow-name}}` is the one placeholder
// `skeleton` substitutes; it can't collide with real workflow syntax
// (no schema key or template looks like it).

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
    # runner: implementer  # uncomment once runners: defines this role
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
    # runner: planner  # uncomment once runners: defines this role
    prompt: \"Plan the work and hand over the tasks document.\"
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    # runner: implementer  # uncomment once runners: defines this role
    depends_on: [plan]
    until: all_tasks_complete
    prompt: \"Read your next task from the tasks document and implement it.\"
";

/// The shape picked off the one list this binary puts choices on —
/// same arrows, same filter, same Escape — or the first shape when
/// nobody picks: a skeleton is a starting point, and "not me, not now"
/// about which one is the plainest one. What the list is for is said
/// above it first, and a console that cannot be drawn on takes the
/// first shape the same way.
fn picked(console: &Console) -> Shape {
    let choices = Shape::all()
        .into_iter()
        .map(|shape| Choice {
            label: Some(shape.does().to_string()),
            ..Choice::named(shape.label(), shape)
        })
        .collect();
    console
        .say("choose a shape")
        .map_err(NoAnswer::from)
        .and_then(|()| choose(console, choices))
        .unwrap_or(Shape::OneNode)
}

/// A safe file stem: letters, digits, `-` and `_` only, non-empty — the
/// same shape a `NodeId`/workflow file name should have, kept narrow so
/// `<name>.yaml` can never escape `.yunta/workflows/`.
fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("workflow name can't be empty".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "workflow name `{name}` must be letters, digits, `-` or `_` only"
        ));
    }
    Ok(())
}

/// The shape a new workflow starts as: the one `--shape` names, or else the
/// one a person picks on the console under `--interactive`, or `one-node`.
async fn choose_shape(shape: Option<&str>, interactive: bool) -> Result<Shape, CliError> {
    match shape {
        Some(raw) => Shape::parse(raw).map_err(CliError::msg),
        // There is no run drawing here, so what opening the console has
        // to say goes out through a door onto nothing, which is stderr.
        None => Ok(match interactive {
            false => Shape::OneNode,
            true => match Console::open(&Diagnostics::none(), Escape::TakesTheFirst).await {
                Some(console) => picked(&console),
                None => {
                    crate::ask::no_terminal("defaulting to `one-node`");
                    Shape::OneNode
                }
            },
        }),
    }
}

pub async fn new_workflow(
    name: &str,
    shape: Option<&str>,
    interactive: bool,
    force: bool,
    look: &Look,
) -> Result<Outcome, CliError> {
    validate_name(name).map_err(CliError::msg)?;

    let shape = choose_shape(shape, interactive).await?;

    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    // Same layered config `yunta check` resolves without an explicit
    // `--config` — an empty/default layer set (no `.yunta/config.yaml`
    // yet, e.g. `new` run before `init`) is a legal, empty `ConfigLayer`,
    // not an error.
    let config = ConfigLayer::merge_layers(
        project::load_named_layers(&cwd)?
            .into_iter()
            .map(|(_, l)| l),
    );
    let lint_declared = config
        .command(&yunta_core::CommandName::from_static("lint"))
        .is_some();

    // Build the real type before writing: a skeleton that doesn't parse as
    // a `Workflow` never reaches disk, so `--force` can't leave an invalid
    // file behind.
    let yaml = shape.skeleton(name, lint_declared);
    let workflow = parse_skeleton(shape, &yaml)?;

    let path = write_skeleton(&cwd, name, &yaml, force).await?;
    let written = Written {
        name,
        shape,
        workflow: &workflow,
        path: &path,
    };
    written.page(&cwd, &config, look).await
}

/// The workflow a `shape` skeleton parses to, or why it does not parse.
fn parse_skeleton(shape: Shape, yaml: &str) -> Result<Workflow, CliError> {
    yunta_core::yaml::parse(yaml).map_err(|e| {
        CliError::msg(format!(
            "the {} skeleton does not parse as a workflow: {e}",
            shape.label()
        ))
    })
}

/// Writes `yaml` as workflow `name` under `cwd` and gives back its path.
async fn write_skeleton(
    cwd: &Path,
    name: &str,
    yaml: &str,
    force: bool,
) -> Result<PathBuf, CliError> {
    let path = cwd.join(".yunta/workflows").join(format!("{name}.yaml"));
    if path.exists() && !force {
        return Err(CliError::msg(format!(
            "{} already exists; pass --force to overwrite",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| CliError::io("create", parent.display(), source))?;
    }
    tokio::fs::write(&path, yaml)
        .await
        .map_err(|source| CliError::io("write", path.display(), source))?;
    Ok(path)
}

/// A workflow `new` wrote: its name, the shape it took, and where.
struct Written<'a> {
    name: &'a str,
    shape: Shape,
    workflow: &'a Workflow,
    path: &'a Path,
}

impl Written<'_> {
    /// The page `new` answers with: the file it wrote, and the verdict
    /// `yunta check` reaches on it, said under it. A workflow the config
    /// cannot run yet is written and said to be one, whatever the config
    /// has to declare first, and the exit says so too.
    async fn page(
        &self,
        cwd: &Path,
        config: &ConfigLayer,
        look: &Look,
    ) -> Result<Outcome, CliError> {
        let at = surco::paths::shown(self.path, cwd, None);
        let found = match self.found(cwd, config).await {
            Ok(found) => found,
            Err(stop) => {
                // The file is on disk whatever stopped the check of it.
                let headline = Headline::new(Mark::Done, file_named(self.path), "written")
                    .fact(self.shape.label());
                document(
                    look,
                    Doc::new()
                        .with(headline)
                        .with(Furrow(Vec::new()))
                        .with(Fields::new().push_if("at", at)),
                );
                return Err(stop);
            }
        };
        let (mark, steps) = match found.errors {
            0 => (Mark::Done, vec![(advice::run(self.name), "runs it")]),
            _ => (Mark::Caution, vec![check_again(&at)]),
        };
        let runs = found.said();
        let Found { blocks, errors } = found;
        let headline = Headline::new(mark, file_named(self.path), "written")
            .fact(self.shape.label())
            .fact(runs);
        document(
            look,
            Doc::new()
                .with(headline)
                .with(Furrow(Vec::new()))
                .with(Fields::new().push_if("at", at))
                .with_all(blocks)
                .with(Next { steps }),
        );
        Ok(match errors {
            0 => Outcome::Success,
            _ => Outcome::Reported,
        })
    }

    /// The verdict `yunta check` reaches on the written file.
    async fn found(&self, cwd: &Path, config: &ConfigLayer) -> Result<Found, CliError> {
        let ctx = crate::context::Context::load()?;
        let verdict = super::verdict::verdict(
            &ctx,
            self.workflow,
            self.path,
            config,
            super::verdict::layer_conflicts(cwd)?,
            super::verdict::Reach::Workflow,
        )
        .await?;
        verdict.found(&ctx).await
    }
}
