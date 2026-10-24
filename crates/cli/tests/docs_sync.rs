//! The documentation is held to the binary. The README's command table
//! names exactly the subcommands `yunta --help` lists; every YAML
//! example under `docs/` and in the README is one the binary accepts; the
//! guide names every tool both MCP servers serve; and every output the
//! docs quote is what the binary prints.
//! A reader never copies something the binary refuses, and a set that
//! grows in one place and not the other fails here, not in a run.

use std::collections::BTreeSet;
use std::path::PathBuf;

use rmcp::ServiceExt;
use yunta_testkit::{
    backticked, fenced_blocks, has_top_level_key, markdown_files, section, stderr, stdout, write,
    yunta_at, yunta_in, Checkout,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A directory the binary runs from and a home of its own beside it, so
/// no invocation in this file reads the developer's `~/.yunta`, the
/// repository's own config or a global git config.
fn elsewhere() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// The subcommands `yunta --help` lists: every row under a group
/// heading — a line of its own ending in `:` — up to the blank line that
/// closes the group. `Options:` and `Examples:` list no subcommand.
fn subcommands_from_help() -> BTreeSet<String> {
    let away = elsewhere();
    let out = yunta_in!(away.path(), &away.path().join("home"), &["--help"]);
    let text = stdout(&out);
    let mut names = BTreeSet::new();
    let mut in_group = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            in_group = false;
            continue;
        }
        if !line.starts_with(' ') && line.ends_with(':') {
            in_group = !matches!(line, "Options:" | "Examples:");
            continue;
        }
        if in_group {
            if let Some(name) = line.split_whitespace().next() {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// The subcommands the README's command table documents: the first word
/// after `yunta` in every row's leading code span.
fn subcommands_from_readme() -> BTreeSet<String> {
    let readme = std::fs::read_to_string(repo_root().join("README.md")).unwrap();
    readme
        .lines()
        .filter_map(|line| line.strip_prefix("| `yunta "))
        .filter_map(|rest| rest.split(|c: char| c.is_whitespace() || c == '`').next())
        .map(str::to_string)
        .collect()
}

#[test]
fn the_readme_command_table_names_exactly_the_subcommands_the_binary_has() {
    let help = subcommands_from_help();
    let readme = subcommands_from_readme();
    assert_eq!(
        readme, help,
        "README command table (left) and `yunta --help` (right) disagree"
    );
}

/// A project every documented workflow can be checked in: the runners
/// the examples name bound to the mock, a suite for their comparisons,
/// the project commands they run,
/// and the catalog the documented composition resolves `use:` against —
/// `build-feature` as the reference writes it, plus the two names it
/// reaches for which no document spells out.
fn check_project() -> Checkout {
    let build_feature = fenced_blocks(&reference(), "yaml")
        .into_iter()
        .find(|block| block.text.lines().any(|line| line == "name: build-feature"))
        .expect("the reference shows the workflow its composition builds with");
    let stub = |name: &str| {
        format!("name: {name}\nnodes:\n  - {{ id: only, kind: bash, run: \"true\" }}\n")
    };
    Checkout::new()
        .config(
            "runners:\n\
             \x20 executor: [{ adapter: mock, model: m }]\n\
             \x20 planner: [{ adapter: mock, model: m }]\n\
             \x20 mechanical: [{ adapter: mock, model: m }]\n\
             \x20 reviewer: [{ adapter: mock, model: m }]\n\
             \x20 reviewer-alt: [{ adapter: mock, model: m }]\nbaseline: { suite: \"true\" }\n\
             commands: { lint: \"true\", fmt: \"true\" }\ncoverage: { cmd: \"true\", threshold: 0 }\n\
             skills: { executors: [{ name: license-scan, kind: binary, path: .yunta/bin/license-scan }] }\n\
             forge: { github: { repo: acme/web, token_env: GITHUB_TOKEN } }\n",
        )
        .file(".yunta/workflows/build-feature.yaml", &build_feature.text)
        .file(
            ".yunta/workflows/design-review.yaml",
            &stub("design-review"),
        )
        .file(".yunta/workflows/qa-review.yaml", &stub("qa-review"))
}

fn check_passes(project: &Checkout, workflow_text: &str, origin: &str) {
    write(&project.repo.join("example.yaml"), workflow_text);
    let out = yunta_at!(project, &["check", "example.yaml"]);
    assert!(
        out.status.success(),
        "{origin}: `yunta check` refused the documented workflow:\n{}{}",
        stdout(&out),
        stderr(&out)
    );
}

/// A documented case runs through `yunta test` in a project that
/// provides the workflow and fixture it names.
fn case_runs(case_text: &str, origin: &str) {
    let workflow = case_text
        .lines()
        .find_map(|line| line.strip_prefix("workflow:"))
        .map(str::trim)
        .unwrap_or_else(|| panic!("{origin}: a case names its workflow"));
    let fixture = case_text
        .lines()
        .find_map(|line| line.strip_prefix("fixture:"))
        .map(str::trim)
        .unwrap_or_else(|| panic!("{origin}: a case names its fixture"));

    let empty_fixture = "dialect: claude-code\nsessions: []\n";
    let project = Checkout::new()
        .file(
            &format!(".yunta/workflows/{workflow}.yaml"),
            &format!("name: {workflow}\nnodes:\n  - {{ id: only, kind: bash, run: \"true\" }}\n"),
        )
        .file(&format!(".yunta/tests/{fixture}"), empty_fixture)
        .file(".yunta/tests/documented.yaml", case_text)
        .committed();

    let out = yunta_at!(project, &["test"]);
    assert!(
        out.status.success(),
        "{origin}: the documented case does not run:\n{}{}",
        stdout(&out),
        stderr(&out)
    );
}

/// The reference is the one document whose job is to show every key with
/// a real value, so the config it shows is one the parser takes
/// — a limit written the way a person reads it is a limit nobody can
/// copy — and every workflow it shows is one `yunta check` accepts,
/// the composition included: its `use:` names three workflows, and a
/// composition whose parts no catalog holds is a shape nobody can run.
#[test]
fn the_reference_config_parses_and_its_workflows_check() {
    let blocks = fenced_blocks(&reference(), "yaml");
    let config = blocks
        .iter()
        .find(|block| has_top_level_key(&block.text, "limits"))
        .expect("the reference shows the config with its limits");
    let _: yunta_core::ConfigLayer = yunta_core::yaml::parse(&config.text)
        .unwrap_or_else(|e| panic!("{}: not a config layer: {e}", config.origin));

    let project = check_project();
    let mut checked = 0;
    for block in blocks
        .iter()
        .filter(|block| has_top_level_key(&block.text, "nodes"))
    {
        check_passes(&project, &block.text, &block.origin);
        checked += 1;
    }
    assert_eq!(
        checked, 2,
        "the reference shows a workflow and the composition built on it"
    );
}

#[test]
fn every_yaml_example_in_the_docs_is_one_the_binary_accepts() {
    let root = repo_root();
    let mut files = vec![root.join("README.md")];
    files.extend(markdown_files(&root.join("docs")));
    let project = check_project();
    let mut seen = 0;
    for file in files {
        for block in fenced_blocks(&file, "yaml") {
            seen += 1;
            let text = &block.text;
            if has_top_level_key(text, "nodes") {
                check_passes(&project, text, &block.origin);
            } else if has_top_level_key(text, "publisher") {
                let manifest: yunta_core::PackManifest = yunta_core::yaml::parse(text)
                    .unwrap_or_else(|e| panic!("{}: {e}", block.origin));
                assert!(manifest.validate().is_empty(), "{}", block.origin);
            } else if has_top_level_key(text, "workflow") && has_top_level_key(text, "expect") {
                case_runs(text, &block.origin);
            } else if has_top_level_key(text, "tasks") {
                yunta_core::shape::read::<yunta_core::TasksFile>(
                    text.as_bytes(),
                    block.origin.clone(),
                )
                .unwrap_or_else(|report| panic!("{report}"));
            } else if has_top_level_key(text, "node_defaults") {
                // A workflow fragment: it is checked inside the smallest
                // workflow that can carry it.
                let embedded = format!(
                    "name: fragment\n{text}nodes:\n  - {{ id: only, kind: bash, run: \"true\" }}\n"
                );
                check_passes(&project, &embedded, &block.origin);
            } else if text.starts_with("- ") {
                // A bare list of nodes: it is checked inside the smallest
                // workflow that can carry it.
                check_passes(
                    &project,
                    &format!("name: fragment\nnodes:\n{text}"),
                    &block.origin,
                );
            } else {
                let _: yunta_core::ConfigLayer = yunta_core::yaml::parse(text)
                    .unwrap_or_else(|e| panic!("{}: not a config layer: {e}", block.origin));
            }
        }
    }
    assert!(
        seen >= 16,
        "the docs carry their YAML examples ({seen} found)"
    );
}

/// The reference: the one document whose job is to show every key of the
/// config and the workflow language with a real value.
fn reference() -> PathBuf {
    repo_root().join("docs/reference.md")
}

/// The guide's MCP section names every tool both servers serve: the tools a
/// run's session is served, a document's submission tool by its
/// `yunta_submit_<kind>` form, and the tools `yunta mcp` serves. A client
/// that calls a tool the guide names gets it, and none is there unnamed.
#[tokio::test]
async fn the_guide_names_every_tool_both_servers_serve() {
    let guide = std::fs::read_to_string(repo_root().join("docs/guide.md")).unwrap();
    let named: BTreeSet<String> = backticked(&section(&guide, "## MCP")).into_iter().collect();
    for tool in yunta_engine::RunTool::all() {
        let name = tool.name();
        let by_form = name.starts_with("yunta_submit_") && named.contains("yunta_submit_<kind>");
        assert!(
            named.contains(name) || by_form,
            "the guide's MCP section names no `{name}`, which a session is served"
        );
    }

    let away = elsewhere();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    yunta_testkit::hermetic(&mut command, away.path(), &away.path().join("home"));
    command.arg("mcp");
    let client = ().serve(rmcp::transport::TokioChildProcess::new(command).unwrap()).await.unwrap();
    let served: Vec<String> = client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();
    client.cancel().await.unwrap();
    assert!(!served.is_empty(), "`yunta mcp` serves tools");
    for name in served {
        assert!(
            named.contains(&name),
            "the guide's MCP section names no `{name}`, which `yunta mcp` serves"
        );
    }
}

/// Every `console` block the documentation quotes: the `$ yunta …` line
/// it opens with, and what follows it.
fn quoted_runs() -> Vec<(String, String, String)> {
    let mut docs = yunta_testkit::markdown_files(&repo_root().join("docs"));
    docs.push(repo_root().join("README.md"));
    docs.iter()
        .flat_map(|path| yunta_testkit::fenced_blocks(path, "console"))
        .filter_map(|block| {
            let (first, printed) = block.text.split_once('\n')?;
            let command = first.strip_prefix("$ yunta ")?.to_string();
            Some((block.origin, command, printed.to_string()))
        })
        .collect()
}

#[test]
fn quoted_output_matches_the_binary() {
    let quoted = quoted_runs();
    assert!(
        !quoted.is_empty(),
        "the documentation quotes no output, so nothing holds it to the binary"
    );
    for (origin, command, printed) in quoted {
        let away = elsewhere();
        let args: Vec<&str> = command.split_whitespace().collect();
        let out = yunta_in!(away.path(), &away.path().join("home"), &args);
        assert_eq!(
            stdout(&out).trim_end(),
            printed.trim_end(),
            "{origin}: `yunta {command}` prints something other than what is quoted"
        );
    }
}
