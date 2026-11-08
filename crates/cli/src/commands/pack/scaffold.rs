//! `yunta pack new`: the skeleton a new pack starts from. Its files are
//! literal templates, parsed back into their types before anything is
//! written, and the new pack is checked and its own case run before the
//! command reports success.

use std::path::Path;

use surco::components::{Fields, Furrow, Headline, Problem};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::look::Look;
use surco::text::counted;
use yunta_core::step::NextStep;
use yunta_core::{ConfigLayer, PackManifest, PackRef, Workflow};
use yunta_render::command;
use yunta_render::page::Page;

use super::audit::{run_pack_tests, PackTestSummary};
use crate::error::CliError;
use crate::output::show;
use crate::output::Outcome;

// Written as literal templates (not built from the typed values) so the
// generated files read exactly as an author would write them; `{{name}}`
// and `{{publisher}}` are the only substitutions, and the manifest and
// workflow are parsed back into their types before anything is written,
// so a skeleton that does not parse never reaches disk.

/// Where a new pack keeps the workflow it starts with.
const EXAMPLE: &str = ".yunta/workflows/example.yaml";

const PACK_MANIFEST_TEMPLATE: &str = r#"name: {{name}}
publisher: {{publisher}}
version: 0.1.0
description: A yunta pack, one verified workflow to build on.
license: Apache-2.0
declares:
  permissions: edit
  network: false
  executors: []
requires:
  runners:
    - name: executor
contents:
  workflows:
    - .yunta/workflows/example.yaml
  docs:
    - README.md
"#;

const PACK_WORKFLOW_TEMPLATE: &str = r#"name: example
description: >-
  The smallest verified shape: a prompt node with its own scope, checked
  by a bash node before the work counts as done. Swap the marker check
  for your own test suite.
nodes:
  - id: work
    kind: prompt
    runner: executor
    scope: ["src/**"]
    prompt: |
      Do the work described here, editing only files under src/. When the
      change is in place, create an empty file at src/.done.
  - id: verify
    kind: bash
    invariant: true
    depends_on: [work]
    run: "test -f src/.done"
"#;

// Self-test config: never shipped in `contents:`, used only when this
// pack's own `.yunta/tests/` run in isolation.
const PACK_SELFTEST_CONFIG: &str = r#"# Self-test config only: runs this pack's own .yunta/tests/ under the
# mock adapter. A project that installs this pack uses its own config.
runners:
  executor:
    - { adapter: mock, model: mock-model }
"#;

const PACK_CASE_TEMPLATE: &str = r#"workflow: example
fixture: fixtures/example.yaml
expect:
  final_state: finished
  nodes:
    work: finished
    verify: finished
"#;

const PACK_FIXTURE_TEMPLATE: &str = r#"dialect: claude-code
effects:
  - { path: src/.done, content: "" }
outcome: { type: completed, summary: "done" }
"#;

const PACK_README_TEMPLATE: &str = r#"# {{publisher}}/{{name}}

A yunta pack scaffolded by `{{pack_new}}`. It ships one workflow,
`example`, and a self-test that runs it under the mock adapter.

## example

A prompt node with a declared `scope`, checked by a bash node before the
work counts as done. Swap the `verify` command for your own test suite.

## Testing

    {{test}}
"#;

/// `template` with what a new pack is called, and how the commands it names are typed.
fn filled(template: &str, pack: &PackRef) -> String {
    template
        .replace("{{publisher}}", pack.publisher().as_str())
        .replace("{{name}}", pack.name().as_str())
        .replace(
            "{{test}}",
            &command::line(&NextStep::Test {
                dir: Some(".".to_string()),
            }),
        )
        .replace(
            "{{pack_new}}",
            &command::line(&NextStep::PackNew { pack: pack.clone() }),
        )
}

/// Scaffolds a new pack at `<cwd>/<name>`: a manifest, one verified
/// workflow, a self-test config and case, and a README. The manifest
/// and workflow are validated by parsing before any file is written,
/// then the pack is checked and its case is run, so a freshly
/// scaffolded pack passes from the first command. Refuses to write
/// over an existing directory.
pub async fn new_pack(pack: &PackRef, look: &Look) -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    let dir = cwd.join(pack.name().as_str());
    if dir.exists() {
        return Err(CliError::msg(format!(
            "{} already exists; choose another name or remove it first",
            dir.display()
        )));
    }

    let fill = |template: &str| filled(template, pack);
    let manifest_yaml = fill(PACK_MANIFEST_TEMPLATE);
    let workflow_yaml = fill(PACK_WORKFLOW_TEMPLATE);

    // Parse the real types first: a skeleton that does not parse is a bug
    // here, never a broken pack on disk.
    let _: PackManifest = yunta_core::yaml::parse(&manifest_yaml)
        .map_err(|e| CliError::msg(format!("the pack manifest skeleton does not parse: {e}")))?;
    let workflow: Workflow = yunta_core::yaml::parse(&workflow_yaml)
        .map_err(|e| CliError::msg(format!("the workflow skeleton does not parse: {e}")))?;

    write_under(&dir, "pack.yaml", &manifest_yaml).await?;
    write_under(&dir, "README.md", &fill(PACK_README_TEMPLATE)).await?;
    write_under(&dir, ".yunta/config.yaml", PACK_SELFTEST_CONFIG).await?;
    write_under(&dir, EXAMPLE, &workflow_yaml).await?;
    write_under(&dir, ".yunta/tests/example.yaml", PACK_CASE_TEMPLATE).await?;
    write_under(
        &dir,
        ".yunta/tests/fixtures/example.yaml",
        PACK_FIXTURE_TEMPLATE,
    )
    .await?;
    let at = surco::paths::shown(&dir, &cwd, None);
    let page = |checked| NewPack {
        pack,
        at: &at,
        checked,
    };
    // The files are on disk whatever stopped the check of them.
    let checked = check_pack(&dir, &workflow)
        .await
        .inspect_err(|_| show(look, &page(None)))?;
    show(look, &page(Some(&checked)));
    Ok(checked.outcome())
}

/// A pack `pack new` wrote, as a person reads it: where it is, and what its
/// check found; the files alone where something stopped the check.
struct NewPack<'a> {
    pack: &'a PackRef,
    /// Where the pack is, as a reader in this directory names it.
    at: &'a str,
    checked: Option<&'a PackCheck>,
}

impl Page for NewPack<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let mark = self.checked.map_or(Mark::Done, PackCheck::mark);
        Doc::new()
            .with(Headline::new(
                mark,
                format!("pack {}", self.pack),
                "written",
            ))
            .with(Furrow(Vec::new()))
            .with(Fields::new().push_if("at", self.at))
            .with_all(self.checked.map(PackCheck::blocks).unwrap_or_default())
    }
}

/// The check `pack new` runs on the pack it wrote: the problems that stop
/// its example workflow, or, when none does, what the pack's own cases say.
enum PackCheck {
    /// The example workflow does not check: each problem, as `check` says it.
    Refused(Vec<Problem>),
    /// It checks, and the pack's cases ran.
    Tested(PackTestSummary),
}

impl PackCheck {
    /// The mark the page opens with: worth a look where the example does not
    /// check, else the case's.
    fn mark(&self) -> Mark {
        match self {
            PackCheck::Refused(_) => Mark::Caution,
            PackCheck::Tested(tests) => tests.mark(),
        }
    }

    /// What the page says of it: how many problems stop the example, and
    /// each; or what the pack's case says.
    fn blocks(&self) -> Vec<Block> {
        match self {
            PackCheck::Refused(problems) => std::iter::once(
                Fields::new()
                    .push_if(
                        "check",
                        format!("{} in {EXAMPLE}", counted(problems.len(), "error")),
                    )
                    .into(),
            )
            .chain(problems.iter().cloned().map(Block::from))
            .collect(),
            PackCheck::Tested(tests) => tests.blocks(),
        }
    }

    /// The exit: reported where the example does not check or a case failed.
    fn outcome(&self) -> Outcome {
        match self {
            PackCheck::Tested(tests) if tests.failed == 0 => Outcome::Success,
            PackCheck::Refused(_) | PackCheck::Tested(_) => Outcome::Reported,
        }
    }
}

/// Writes one file of the scaffold under `dir`, creating its directory
/// first, and names the path it could not write.
async fn write_under(dir: &Path, rel: &str, contents: &str) -> Result<(), CliError> {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|s| CliError::io("create", parent.display(), s))?;
    }
    tokio::fs::write(&path, contents)
        .await
        .map_err(|s| CliError::io("write", path.display(), s))
}

/// Checks the scaffolded workflow against the pack's own self-test config,
/// and runs the pack's case when the check passes. A check that fails
/// reports every problem and runs nothing.
async fn check_pack(dir: &Path, workflow: &Workflow) -> Result<PackCheck, CliError> {
    let config = ConfigLayer::merge_layers(
        crate::project::load_named_layers(dir)?
            .into_iter()
            .map(|(_, layer)| layer),
    );
    let facts = crate::commands::declared_facts(&config);
    let errors = yunta_engine::check(workflow, &config, &facts);
    if !errors.is_empty() {
        let problems = errors
            .iter()
            .map(|error| crate::error::page::refused(error, None, EXAMPLE, None))
            .collect();
        return Ok(PackCheck::Refused(problems));
    }
    let interrupt = crate::interrupt::Interrupt::ctrl_c()
        .map_err(|source| CliError::io("install the interrupt handler for", "pack new", source))?;
    Ok(PackCheck::Tested(run_pack_tests(dir, interrupt).await))
}
