//! The documentation is verified against the binary: the README's
//! command table names exactly the subcommands `yunta --help` lists,
//! every YAML example in `docs/` and the README parses — and, for a
//! workflow, passes `yunta check` — so a reader never copies something
//! the binary refuses, and the tasks spec states every rule the engine
//! holds a tasks document to.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use yunta_testkit::{stderr, stdout, write, yunta_at, yunta_in, Checkout};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A directory the binary runs from and a home of its own beside it, so
/// no invocation in this file reads the developer's `~/.yunta`, the
/// repository's own config or a global git config.
fn elsewhere() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// The subcommands `yunta --help` lists under `Commands:`.
fn subcommands_from_help() -> BTreeSet<String> {
    let away = elsewhere();
    let out = yunta_in!(away.path(), &away.path().join("home"), &["--help"]);
    let text = stdout(&out);
    let mut names = BTreeSet::new();
    let mut in_commands = false;
    for line in text.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if in_commands {
            if line.trim().is_empty() {
                break;
            }
            if let Some(name) = line.split_whitespace().next() {
                if name != "help" {
                    names.insert(name.to_string());
                }
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

/// One fenced ```yaml block, with where it came from.
struct Block {
    origin: String,
    text: String,
}

fn yaml_blocks(path: &Path) -> Vec<Block> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut blocks = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (index, line) in text.lines().enumerate() {
        match &mut current {
            None if line.trim_start().starts_with("```yaml") => {
                current = Some((index + 1, String::new()));
            }
            Some((start, body)) if line.trim_start().starts_with("```") => {
                blocks.push(Block {
                    origin: format!("{}:{}", path.display(), start),
                    text: std::mem::take(body),
                });
                current = None;
            }
            Some((_, body)) => {
                body.push_str(line);
                body.push('\n');
            }
            None => {}
        }
    }
    blocks
}

fn has_top_level_key(text: &str, key: &str) -> bool {
    text.lines()
        .any(|line| line.starts_with(key) && line[key.len()..].starts_with(':'))
}

/// A project every documented workflow can be checked in: the runners
/// the examples name, bound to the mock.
fn check_project() -> Checkout {
    Checkout::new().config(
        "runners:\n\
         \x20 executor: [{ adapter: mock, model: m }]\n\
         \x20 planner: [{ adapter: mock, model: m }]\n\
         \x20 mechanical: [{ adapter: mock, model: m }]\n\
         \x20 reviewer: [{ adapter: mock, model: m }]\n\
         \x20 reviewer-alt: [{ adapter: mock, model: m }]\n",
    )
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

    let project = Checkout::new()
        .file(
            &format!(".yunta/workflows/{workflow}.yaml"),
            &format!("name: {workflow}\nnodes:\n  - {{ id: only, kind: bash, run: \"true\" }}\n"),
        )
        .file(&format!(".yunta/tests/{fixture}"), "sessions: []\n")
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

/// The reference schema is the one document whose job is to show every
/// key with a real value, so every value in it has to be one the parser
/// takes — a limit written the way a person reads it is a limit nobody
/// can copy.
#[test]
fn the_reference_schema_config_parses() {
    let file = repo_root().join("docs/design/referencia-schema.md");
    let blocks = yaml_blocks(&file);
    let config = blocks
        .iter()
        .find(|block| has_top_level_key(&block.text, "limits"))
        .expect("the reference schema shows the config with its limits");
    let _: yunta_core::ConfigLayer = yunta_core::yaml::parse(&config.text)
        .unwrap_or_else(|e| panic!("{}: not a config layer: {e}", config.origin));
}

#[test]
fn every_yaml_example_in_the_docs_is_one_the_binary_accepts() {
    let root = repo_root();
    let mut files = vec![root.join("README.md")];
    for entry in std::fs::read_dir(root.join("docs")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "md") {
            files.push(path);
        }
    }
    let project = check_project();
    let mut seen = 0;
    for file in files {
        for block in yaml_blocks(&file) {
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
            } else if has_top_level_key(text, "node_defaults") {
                // A workflow fragment: it is checked inside the smallest
                // workflow that can carry it.
                let embedded = format!(
                    "name: fragment\n{text}nodes:\n  - {{ id: only, kind: bash, run: \"true\" }}\n"
                );
                check_passes(&project, &embedded, &block.origin);
            } else {
                let _: yunta_core::ConfigLayer = yunta_core::yaml::parse(text)
                    .unwrap_or_else(|e| panic!("{}: not a config layer: {e}", block.origin));
            }
        }
    }
    assert!(
        seen >= 8,
        "the docs carry their YAML examples ({seen} found)"
    );
}

/// The body of a top-level section, from its heading to the next one.
fn section(text: &str, heading: &str) -> String {
    let mut body = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("## ") {
            if inside {
                break;
            }
            inside = line.starts_with(heading);
            continue;
        }
        if inside {
            body.push_str(line);
            body.push('\n');
        }
    }
    body
}

fn is_numbered(line: &str) -> bool {
    line.split_once(". ")
        .is_some_and(|(number, _)| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

/// The numbered items of a section, each with its continuation lines —
/// the indented ones that follow it, up to the blank line that ends it.
fn numbered_items(body: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut open = false;
    for line in body.lines() {
        if is_numbered(line) {
            items.push(line.to_string());
            open = true;
        } else if open && line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
            let item = items.last_mut().expect("a continuation follows an item");
            item.push(' ');
            item.push_str(line.trim());
        } else {
            open = false;
        }
    }
    items
}

/// The rule codes `text` names in backticks, in the spelling the engine
/// publishes them by.
fn rule_codes_named(text: &str) -> Vec<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| {
            yunta_core::diagnostic::RuleCode::ALL
                .iter()
                .any(|code| code.as_str() == *span)
        })
        .map(str::to_string)
        .collect()
}

/// The rules of the tasks document are published before it is written —
/// the same list the contract hands a session — so the spec that states
/// them and the engine that enforces them are one list read twice. A rule
/// the spec leaves out is one a writer meets for the first time as a
/// failure.
#[test]
fn the_tasks_spec_states_every_rule_the_engine_publishes() {
    let spec = repo_root().join("docs/design/spec-tasks.md");
    let text = std::fs::read_to_string(&spec).unwrap();
    let items = numbered_items(&section(&text, "## 3."));
    let stated: BTreeSet<String> = items
        .iter()
        .map(|item| {
            let named = rule_codes_named(item);
            assert_eq!(
                named.len(),
                1,
                "every item of §3 names the one rule code it states, in backticks: {item}"
            );
            named[0].clone()
        })
        .collect();

    let published: BTreeSet<String> = <yunta_core::TasksFile as yunta_core::shape::Document>::RULES
        .iter()
        .map(|rule| rule.code.as_str().to_string())
        .collect();

    assert_eq!(
        stated, published,
        "§3 of the tasks spec (left) and the rules the engine publishes (right) disagree"
    );
    assert_eq!(
        items.len(),
        published.len(),
        "§3 states one item per published rule"
    );
}
