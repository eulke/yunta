//! `yunta new <name> [--shape ...]`: writes
//! `.yunta/workflows/<name>.yaml` from a minimal, commented schema
//! skeleton — closer to `cargo new` than to a working workflow: ruled
//! paper to edit, not a working pipeline. Never references a pack and
//! never touches `yunta.lock` — `new` creates the team's own content,
//! `pack add` is the only verb that brings in someone else's, and the
//! two stay disjoint on purpose. Runs `check` on what it wrote and
//! reports the result, same as `yunta check` would.

use std::io::IsTerminal;

use yunta_core::text::problems;
use yunta_core::{ConfigLayer, Workflow};

use crate::error::{note, warn, CliError, Outcome};
use crate::project;

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
            .ok_or_else(|| format!("unknown shape `{name}` — choose one of: {}", Self::listed()))
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

    fn skeleton(&self, name: &str) -> String {
        let template = match self {
            Self::OneNode => ONE_NODE_TEMPLATE,
            Self::LintFix => LINT_FIX_TEMPLATE,
            Self::Tasks => TASKS_TEMPLATE,
        };
        template.replace("{{workflow-name}}", name)
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
# verification — replace `run:` with something that actually checks
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
# `on_failure.goto` re-routes to `fix`, which gets an
# agent session to address what's wrong, then control returns to
# `lint` to re-verify. `max_reroutes` caps how many correction
# attempts run before escalating to a person.
nodes:
  - id: lint
    kind: bash
    run: \"true\"  # replace with your real lint/test command
    on_failure: { goto: fix, max_reroutes: 2 }
  - id: fix
    kind: prompt
    # runner: implementer  # uncomment once runners: defines this role
    prompt: \"Fix what `lint` reported.\"
";

const TASKS_TEMPLATE: &str = "\
name: {{workflow-name}}
# A tasks cycle, empty to start: `plan` opens an agent session
# that writes a tasks document artifact; `implement` loops
# over it, dispatching one mechanically-verified session per ready
# task, until every task is `done`.
nodes:
  - id: plan
    kind: prompt
    # runner: planner  # uncomment once runners: defines this role
    prompt: \"Write a tasks document to {{run.dir}}/artifacts/ledger.yaml.\"
    artifacts:
      produces:
        - { name: ledger.yaml, kind: tasks }
  - id: implement
    kind: loop
    # runner: implementer  # uncomment once runners: defines this role
    depends_on: [plan]
    until: all_tasks_complete
    prompt: \"Read your next task from the tasks document and implement it.\"
";

fn prompt_shape() -> Shape {
    println!("choose a shape:");
    for (i, shape) in Shape::all().iter().enumerate() {
        println!("  {}) {}", i + 1, shape.label());
    }
    print!("shape [1]: ");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
        return Shape::OneNode;
    }
    match line.trim() {
        "2" => Shape::LintFix,
        "3" => Shape::Tasks,
        _ => Shape::OneNode,
    }
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

pub fn new_workflow(
    name: &str,
    shape: Option<&str>,
    interactive: bool,
    force: bool,
) -> Result<Outcome, CliError> {
    validate_name(name).map_err(CliError::msg)?;

    let shape = match shape {
        Some(raw) => Shape::parse(raw).map_err(CliError::msg)?,
        None if interactive && std::io::stdin().is_terminal() => prompt_shape(),
        None => {
            if interactive {
                warn("--interactive given but stdin isn't a TTY — defaulting to `one-node`");
            }
            Shape::OneNode
        }
    };

    // Build the real type before writing: a skeleton that doesn't parse as
    // a `Workflow` never reaches disk, so `--force` can't leave an invalid
    // file behind.
    let yaml = shape.skeleton(name);
    let workflow: Workflow = yunta_core::yaml::parse(&yaml).map_err(|e| {
        CliError::msg(format!(
            "the {} skeleton does not parse as a workflow: {e}",
            shape.label()
        ))
    })?;

    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    let path = cwd.join(".yunta/workflows").join(format!("{name}.yaml"));
    if path.exists() && !force {
        return Err(CliError::msg(format!(
            "{} already exists — pass --force to overwrite",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|source| CliError::io("create", parent.display(), source))?;
    }
    std::fs::write(&path, &yaml).map_err(|source| CliError::io("write", path.display(), source))?;
    println!("wrote {} ({})", path.display(), shape.label());

    // Same layered config `yunta check` resolves without an explicit
    // `--config` — an empty/default layer set (no `.yunta/config.yaml`
    // yet, e.g. `new` run before `init`) is a legal, empty `ConfigLayer`,
    // not an error: these skeletons never reference a `runner:`
    // precisely so `check` never depends on that config existing.
    let config = ConfigLayer::merge_layers(
        project::load_named_layers(&cwd)?
            .into_iter()
            .map(|(_, l)| l),
    );

    let errors = yunta_engine::check(&workflow, &config);
    if errors.is_empty() {
        println!("{}: OK", path.display());
        Ok(Outcome::Success)
    } else {
        note(problems(path.display(), &errors));
        Ok(Outcome::Reported)
    }
}
