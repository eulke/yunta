//! The documentation is held to the binary. The README's command table
//! names exactly the subcommands `yunta --help` lists; every YAML
//! example under `docs/` and in the README is one the binary accepts;
//! and every closed set a document states — the event kinds and the
//! fields of each payload, the capabilities an adapter may declare and
//! what each built adapter does declare, the tools of both MCP
//! surfaces, the rules of a tasks document, the check builtins, the
//! context sources and the input types — is the set the types publish.
//! A reader never copies something the binary refuses, and a set that
//! grows in one place and not the other fails here, not in a run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rmcp::ServiceExt;
use yunta_testkit::{
    backticked, bullets, fenced_blocks, field_tables, fixed_consts, has_top_level_key, json_schema,
    markdown_files, names_after, number_before, numbered_items, rule_codes_named, section, stderr,
    stdout, struct_fields, table_rows, tagged_variants, write, yunta_at, yunta_in, Checkout,
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

/// A project every documented workflow can be checked in: the runners
/// the examples name, bound to the mock, and the catalog the documented
/// composition resolves its `use:` against — `build-feature` as the
/// reference itself writes it, and the two names that composition
/// reaches for which no document spells out.
fn check_project() -> Checkout {
    let build_feature = fenced_blocks(&reference_schema(), "yaml")
        .into_iter()
        .find(|block| block.text.starts_with("name: build-feature"))
        .expect("the reference schema shows the workflow its composition builds with");
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
             \x20 reviewer-alt: [{ adapter: mock, model: m }]\n",
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
/// key with a real value, so the config it shows is one the parser takes
/// — a limit written the way a person reads it is a limit nobody can
/// copy — and every workflow it shows is one `yunta check` accepts,
/// the composition included: its `use:` names three workflows, and a
/// composition whose parts no catalog holds is a shape nobody can run.
#[test]
fn the_reference_config_parses_and_its_workflows_check() {
    let blocks = fenced_blocks(&reference_schema(), "yaml");
    let config = blocks
        .iter()
        .find(|block| has_top_level_key(&block.text, "limits"))
        .expect("the reference schema shows the config with its limits");
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
        "the reference schema shows a workflow and the composition built on it"
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

/// The reference schema: the one document whose job is to show every
/// key of the language with a real value.
fn reference_schema() -> PathBuf {
    repo_root().join("docs/design/referencia-schema.md")
}

/// One of the design documents, by file name.
fn design_doc(name: &str) -> String {
    let path = repo_root().join("docs/design").join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read `{}`: {e}", path.display()))
}

/// The contract's event table is where a reader learns what a run
/// writes, and the payload spec counts that table's rows and kinds to
/// say how many sections it owes: one closed set stated in three places,
/// which is one statement or a contradiction.
#[test]
fn the_contract_event_table_names_exactly_the_kinds_the_binary_writes() {
    let rows = table_rows(&section(&design_doc("contrato-del-run.md"), "# 3."));
    let named: BTreeSet<String> = rows.iter().flat_map(|row| backticked(&row[0])).collect();
    let published: BTreeSet<String> = yunta_core::events::EventPayload::KINDS
        .iter()
        .map(|kind| kind.to_string())
        .collect();
    assert_eq!(
        named, published,
        "the contract's event table (left) and the kinds the binary writes (right) disagree"
    );

    let counts = section(&design_doc("spec-events.md"), "## 0.");
    assert_eq!(
        Some(rows.len()),
        number_before(&counts, "filas"),
        "the payload spec counts the contract's rows"
    );
    assert_eq!(
        Some(named.len()),
        number_before(&counts, "`kind`"),
        "the payload spec counts the contract's kinds"
    );
}

/// The payload spec is the field-by-field statement of what the log
/// carries, so a section that omits a field, or calls an optional one
/// mandatory, is a promise the log does not keep — and a reader who
/// writes against it reads an event that has more or less than it says.
#[test]
fn every_event_spec_section_lists_the_fields_its_payload_has() {
    let carried = tagged_variants(
        &repo_root().join("crates/core/schemas/events.json"),
        "EventPayload",
    );
    let stated = field_tables(&design_doc("spec-events.md"), "### 5");
    assert_eq!(
        stated.keys().collect::<BTreeSet<_>>(),
        carried.keys().collect::<BTreeSet<_>>(),
        "§5 of the payload spec (left) covers every kind the log carries (right)"
    );
    for (kind, fields) in &carried {
        assert_eq!(
            stated.get(kind),
            Some(fields),
            "§5's section for `{kind}` (left) and the payload the log carries (right) disagree"
        );
    }
}

/// Adapters are heterogeneous on purpose and the engine asks before it
/// relies on anything, so the type that says what an adapter may declare
/// and the table that says what each absence costs are one closed set
/// read twice. A capability missing from either is one the engine
/// consults with nothing written about it.
#[test]
fn the_adapter_spec_lists_every_capability_and_its_degradation() {
    let spec = design_doc("spec-adapter.md");
    let published: BTreeSet<String> = yunta_core::Capability::ALL
        .iter()
        .map(|capability| capability.as_str().to_string())
        .collect();

    let declared: BTreeSet<String> = struct_fields(&section(&spec, "## 2."), "Capabilities")
        .into_iter()
        .collect();
    assert_eq!(
        declared, published,
        "§2's `Capabilities` (left) and the capabilities an adapter declares (right) disagree"
    );

    let degraded: BTreeSet<String> = table_rows(&section(&spec, "## 5."))
        .iter()
        .flat_map(|row| backticked(&row[0]))
        .collect();
    assert_eq!(
        degraded, published,
        "§5's degradation table (left) and the capabilities an adapter declares (right) disagree"
    );
}

/// The adapter spec is what a team reads before pointing a runner at a
/// CLI, and the engine plans every node against what the adapter
/// declares. A value the spec states and the adapter does not declare is
/// a promise the run breaks in the middle of a node.
#[test]
fn every_built_adapter_declares_what_the_spec_says_it_declares() {
    let spec = design_doc("spec-adapter.md");
    let settings = yunta_core::AdapterSettings::default();
    let built: Vec<Box<dyn yunta_core::port::Adapter>> = vec![
        Box::new(yunta_adapters::ClaudeCodeAdapter::new(&settings)),
        Box::new(yunta_adapters::CodexAdapter::new(&settings)),
    ];
    for adapter in built {
        let declared = serde_json::to_value(adapter.capabilities()).unwrap();
        let publishes: BTreeMap<String, String> = yunta_core::Capability::ALL
            .iter()
            .map(|capability| {
                let value = &declared[capability.as_str()];
                let shown = value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_string);
                (capability.as_str().to_string(), shown)
            })
            .collect();

        let heading = format!("### `{}`", adapter.id());
        let stated: BTreeMap<String, String> = table_rows(&section(&spec, &heading))
            .iter()
            .filter_map(|row| Some((backticked(&row[0]).pop()?, backticked(&row[1]).pop()?)))
            .collect();
        assert_eq!(
            stated,
            publishes,
            "§6's table for `{}` (left) and what the adapter declares (right) disagree",
            adapter.id()
        );
    }
}

/// The contract is the only statement of what a client may call, and
/// both surfaces are closed sets the binary builds: an agent that reads
/// the contract and calls a tool it names gets an error from a server
/// that never had it, and one the contract omits is a capability nobody
/// knows is there.
#[tokio::test]
async fn the_contract_names_every_control_plane_tool() {
    let contract = design_doc("contrato-del-run.md");

    let per_run: BTreeSet<String> = backticked(&section(&contract, "### MCP por-run"))
        .into_iter()
        .filter(|name| name.starts_with("yunta_"))
        .collect();
    let mounted: BTreeSet<String> = yunta_engine::RunTool::all()
        .into_iter()
        .map(|tool| tool.name().to_string())
        .collect();
    assert_eq!(
        per_run, mounted,
        "§6.4's per-run tools (left) and the tools a session is served (right) disagree"
    );

    let away = elsewhere();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    yunta_testkit::hermetic(&mut command, away.path(), &away.path().join("home"));
    command.arg("mcp");
    let client = ().serve(rmcp::transport::TokioChildProcess::new(command).unwrap()).await.unwrap();
    let served: BTreeSet<String> = client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();
    client.cancel().await.unwrap();

    assert_eq!(
        names_after(&contract, "### Superficie de control", "Tools: "),
        served,
        "§6.4's control-plane tools (left) and the tools `yunta mcp` serves (right) disagree"
    );
}

/// Three closed sets of the workflow language the contract states in
/// prose: the builtins a `check` may name, the keys a `context:` entry
/// opens with, and the types an input declares. A name the contract
/// lists and the language does not take is a workflow nobody can write.
#[test]
fn the_contract_closed_sets_match_the_types() {
    let contract = design_doc("contrato-del-run.md");
    let schema = json_schema(&repo_root().join("crates/core/schemas/workflow.json"));

    let check = schema["$defs"]["Node"]["oneOf"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|branch| branch["properties"]["kind"]["const"] == "check")
        .expect("`check` is one of the kinds a node has");
    let builtins: BTreeSet<String> = bullets(&section(&contract, "## 7.1"))
        .iter()
        .filter_map(|item| backticked(item).into_iter().next())
        .collect();
    assert_eq!(
        builtins,
        fixed_consts(&check["oneOf"], "builtin"),
        "§7.1's builtins (left) and the builtins a `check` takes (right) disagree"
    );

    let sources: BTreeSet<String> = schema["$defs"]["ContextSpec"]["anyOf"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|one| one["required"][0].as_str().map(str::to_string))
        .collect();
    assert_eq!(
        names_after(&contract, "# 9.", "Builtin: "),
        sources,
        "§9's builtin sources (left) and the keys a `context:` entry opens with (right) disagree"
    );

    assert_eq!(
        names_after(&contract, "## 2.3", "Tipos: "),
        fixed_consts(&schema["$defs"]["InputSpec"]["oneOf"], "type"),
        "§2.3's input types (left) and the types an input declares (right) disagree"
    );
}
