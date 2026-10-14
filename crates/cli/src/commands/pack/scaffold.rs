//! `yunta pack new`: the skeleton a new pack starts from. Its files are
//! literal templates, parsed back into their types before anything is
//! written, and the new pack is checked and its own case run before the
//! command reports success.

use std::path::Path;

use yunta_core::text::problems;
use yunta_core::{ConfigLayer, PackManifest, PackRef, Workflow};

use crate::error::{note, CliError, Outcome};
use crate::output::say;
use crate::render::Look;

// Written as literal templates (not built from the typed values) so the
// generated files read exactly as an author would write them; `{{name}}`
// and `{{publisher}}` are the only substitutions, and the manifest and
// workflow are parsed back into their types before anything is written,
// so a skeleton that does not parse never reaches disk.

const PACK_MANIFEST_TEMPLATE: &str = r#"name: {{name}}
publisher: {{publisher}}
version: 0.1.0
description: A yunta pack — one verified workflow to build on.
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
const PACK_SELFTEST_CONFIG: &str = r#"# Self-test config only — runs this pack's own .yunta/tests/ under the
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

A yunta pack scaffolded by `yunta pack new`. It ships one workflow,
`example`, and a self-test that runs it under the mock adapter.

## example

A prompt node with a declared `scope`, checked by a bash node before the
work counts as done. Swap the `verify` command for your own test suite.

## Testing

    yunta test --dir .
"#;

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
            "{} already exists — choose another name or remove it first",
            dir.display()
        )));
    }

    let fill = |template: &str| {
        template
            .replace("{{publisher}}", pack.publisher().as_str())
            .replace("{{name}}", pack.name().as_str())
    };
    let manifest_yaml = fill(PACK_MANIFEST_TEMPLATE);
    let workflow_yaml = fill(PACK_WORKFLOW_TEMPLATE);

    // Parse the real types first: a skeleton that does not parse is a bug
    // here, never a broken pack on disk.
    let _: PackManifest = yunta_core::yaml::parse(&manifest_yaml)
        .map_err(|e| CliError::msg(format!("the pack manifest skeleton does not parse: {e}")))?;
    let workflow: Workflow = yunta_core::yaml::parse(&workflow_yaml)
        .map_err(|e| CliError::msg(format!("the workflow skeleton does not parse: {e}")))?;

    let write = |rel: &str, contents: &str| -> Result<(), CliError> {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|s| CliError::io("create", parent.display(), s))?;
        }
        std::fs::write(&path, contents).map_err(|s| CliError::io("write", path.display(), s))
    };
    write("pack.yaml", &manifest_yaml)?;
    write("README.md", &fill(PACK_README_TEMPLATE))?;
    write(".yunta/config.yaml", PACK_SELFTEST_CONFIG)?;
    write(".yunta/workflows/example.yaml", &workflow_yaml)?;
    write(".yunta/tests/example.yaml", PACK_CASE_TEMPLATE)?;
    write(".yunta/tests/fixtures/example.yaml", PACK_FIXTURE_TEMPLATE)?;
    say(look, format!("wrote {}", dir.display()));

    verify_scaffold(&dir, &workflow, look).await
}

/// Checks the scaffolded workflow against the pack's own self-test config,
/// and runs the pack's case when the check passes. A check that fails
/// reports every problem and runs nothing.
async fn verify_scaffold(
    dir: &Path,
    workflow: &Workflow,
    look: &Look,
) -> Result<Outcome, CliError> {
    let config = ConfigLayer::merge_layers(
        crate::project::load_named_layers(dir)?
            .into_iter()
            .map(|(_, layer)| layer),
    );
    let facts = crate::commands::declared_facts(&config);
    let errors = yunta_engine::check(workflow, &config, &facts);
    if !errors.is_empty() {
        let words: Vec<String> = errors.iter().map(crate::error::words::check).collect();
        note(problems(
            dir.join(".yunta/workflows/example.yaml").display(),
            &words,
        ));
        return Ok(Outcome::Reported);
    }
    crate::commands::test::test(Some(dir), look).await
}
