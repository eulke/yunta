//! See [`super`]. The skeleton a new pack starts from. Its files are literal
//! templates, parsed back into their types before anything is written, and
//! the new pack's example workflow is checked against the pack's own
//! self-test config once it is on disk.

use std::path::{Path, PathBuf};

use yunta_core::refusal::PackRefusal;
use yunta_core::step::NextStep;
use yunta_core::{ConfigLayer, PackManifest, PackRef, Workflow};
use yunta_render::command;
use yunta_render::page::pack::NewPackCheck;

use super::{refused, Packs};
use crate::{Error, Failure, Remarks, Stop, Workspace};

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

impl<'w> Packs<'w> {
    /// Scaffolds a new pack at `<dir>/<name>`, beside the repository's own
    /// files: a manifest, one verified workflow, a self-test config and case,
    /// and a README. The manifest and the workflow are parsed before any file
    /// is written, so a skeleton that does not parse never reaches disk.
    /// Refused where a directory already stands.
    pub async fn scaffold(&self, pack: &PackRef) -> Result<ScaffoldedPack<'w>, Error> {
        let dir = self.workspace.dir.join(pack.name().as_str());
        if dir.exists() {
            return Err(refused(PackRefusal::DirectoryExists { dir }));
        }
        let fill = |template: &str| filled(template, pack);
        let manifest_yaml = fill(PACK_MANIFEST_TEMPLATE);
        let workflow_yaml = fill(PACK_WORKFLOW_TEMPLATE);
        // Parse the real types first: a skeleton that does not parse is a
        // bug here, never a broken pack on disk.
        PackManifest::read(&manifest_yaml).map_err(|e| unparsed("the pack manifest", e))?;
        let workflow = Workflow::parse(&workflow_yaml).map_err(|e| unparsed("the workflow", e))?;
        write_under(&dir, "pack.yaml", &manifest_yaml).await?;
        write_under(&dir, "README.md", &fill(PACK_README_TEMPLATE)).await?;
        write_under(&dir, ".yunta/config.yaml", PACK_SELFTEST_CONFIG).await?;
        write_under(&dir, EXAMPLE, &workflow_yaml).await?;
        write_under(&dir, ".yunta/tests/example.yaml", PACK_CASE_TEMPLATE).await?;
        let fixture = ".yunta/tests/fixtures/example.yaml";
        write_under(&dir, fixture, PACK_FIXTURE_TEMPLATE).await?;
        Ok(ScaffoldedPack {
            workspace: self.workspace,
            dir,
            workflow,
        })
    }
}

/// A new pack's skeleton, written: where it is, and the example workflow it
/// starts with.
pub struct ScaffoldedPack<'w> {
    workspace: &'w Workspace,
    /// Where the pack's files are.
    pub dir: PathBuf,
    workflow: Workflow,
}

impl ScaffoldedPack<'_> {
    /// Checks the example workflow against the pack's own self-test config:
    /// the refusal, with every problem in it, when it does not check; when
    /// it does, the pack's own case run, so a freshly scaffolded pack passes
    /// from the first command. What the case's run says is said through
    /// `remarks`.
    pub async fn check(&self, remarks: &dyn Remarks, stop: &Stop) -> Result<NewPackCheck, Error> {
        let layers = crate::config_layers(&self.dir, &self.workspace.env)?;
        let config = ConfigLayer::merge_layers(layers.into_iter().map(|(_, layer)| layer));
        let facts = crate::machine::declared_facts(&config);
        let errors = yunta_engine::check(&self.workflow, &config, &facts);
        if errors.is_empty() {
            let cases = self.workspace.cases_run(&self.dir, remarks, stop).await;
            return Ok(NewPackCheck::Tested(cases));
        }
        let problems = errors
            .iter()
            .map(|error| yunta_render::refusal::check::problem(error, None, EXAMPLE, None))
            .collect();
        Ok(NewPackCheck::Refused {
            workflow: EXAMPLE.to_string(),
            problems,
        })
    }
}

/// Writes one file of the scaffold under `dir`, creating its directory
/// first, and names the path it could not write.
async fn write_under(dir: &Path, rel: &str, contents: &str) -> Result<(), Error> {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| unwritten("create", parent, source))?;
    }
    tokio::fs::write(&path, contents)
        .await
        .map_err(|source| unwritten("write", &path, source))
}

/// A skeleton of `what` that does not parse back into its type.
fn unparsed(what: &'static str, said: impl std::fmt::Display) -> Error {
    Failure::new(SkeletonUnparsed {
        what,
        said: said.to_string(),
    })
    .into()
}

/// A file or a directory of the skeleton that could not be made.
fn unwritten(verb: &'static str, path: &Path, source: std::io::Error) -> Error {
    Failure::new(ScaffoldUnwritten {
        verb,
        path: path.to_path_buf(),
        source,
    })
    .into()
}

/// A template of the skeleton that does not parse: a bug in this binary,
/// caught before anything is written.
#[derive(Debug, thiserror::Error)]
#[error("{what} skeleton does not parse: {said}")]
struct SkeletonUnparsed {
    what: &'static str,
    said: String,
}

/// A file or a directory of the skeleton the disk would not take.
#[derive(Debug, thiserror::Error)]
#[error("failed to {verb} {}: {source}", path.display())]
struct ScaffoldUnwritten {
    verb: &'static str,
    path: PathBuf,
    source: std::io::Error,
}
