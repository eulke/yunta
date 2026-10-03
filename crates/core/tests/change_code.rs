//! What a change's code has to show a person reviewing a plan: each
//! symbol its `at` names, and code rather than a sentence about it.

use yunta_core::TasksFile;

/// The codes of the rules about change code that `plan` breaks, in order.
fn broken(plan: &str) -> Vec<String> {
    let tasks: TasksFile = yunta_core::yaml::parse(plan).expect("the plan reads");
    tasks
        .unexplained()
        .iter()
        .filter_map(|diagnostic| match &diagnostic.problem {
            yunta_core::diagnostic::Problem::Rule { code, .. } => {
                Some(code.to_string()).filter(|code| code.starts_with("change-code"))
            }
            _ => None,
        })
        .collect()
}

/// A plan of one task whose changes are `changes`, with `shapes` beside it.
fn plan(changes: &str, shapes: &str) -> String {
    format!(
        r##"
summary: "Install packs for the user"
description: "A pack can be installed for the user."
{shapes}
tasks:
  - id: global-pack-store
    title: "Add the global store"
    description: "Adds the store."
    outcome: "A global pack installs for the user"
    scope: [crates/cli/src/pack.rs, crates/cli/src/cli/mod.rs, crates/cli/src/commands/pack.rs, crates/cli/src/commands/list/mod.rs, docs/packs.md]
    criteria:
      - {{ cmd: "cargo test -p yunta --test global_pack_management_cmd", proves: "it installs for the user" }}
    changes:
{changes}
"##
    )
}

#[test]
fn a_change_whose_code_leaves_out_what_its_at_names_is_refused() {
    let changes = r##"      - { at: "crates/cli/src/commands/pack.rs::add/update/remove/list", what: "route each command", code: "pub async fn add(source: &str, global: bool) -> Result<Outcome, CliError>;" }
      - { at: "crates/cli/src/cli/mod.rs::PackAction", what: "the flag", code: "#[arg(long)] global: bool," }
      - { at: "crates/cli/src/commands/list/mod.rs::list_workflows", what: "list globals", code: "// Render a selected global pack with an explicit global scope label." }"##;

    assert_eq!(
        broken(&plan(changes, "")),
        [
            "change-code-misses-a-name",
            "change-code-misses-a-name",
            "change-code-misses-a-name",
            "change-code-is-a-comment",
        ]
    );
}

#[test]
fn a_change_whose_code_shows_every_name_it_gives_is_accepted() {
    let changes = r##"      - at: "crates/cli/src/commands/pack.rs::add/update"
        what: "route each command"
        code: |-
          pub async fn add(source: &str, global: bool) -> Result<Outcome, CliError>;
          pub async fn update(pack: &str, global: bool) -> Result<Outcome, CliError>;
      - at: "crates/cli/src/cli/mod.rs::PackAction"
        what: "the flag"
        code: |-
          #[derive(clap::Subcommand)]
          pub enum PackAction {
              Add { source: String, #[arg(long)] global: bool },
          }
      - { at: docs/packs.md, what: "the guide", code: "# Global packs" }
      - { at: src/lib.rs, what: "an empty library", code: "//! The demo library." }"##;

    assert_eq!(broken(&plan(changes, "")), Vec::<String>::new());
}

#[test]
fn a_shape_the_task_declares_in_the_file_shows_the_names_its_change_leaves_out() {
    let changes = r##"      - { at: "crates/cli/src/pack.rs::PackStore", what: "the store", code: "pub fn global_pack_store(user_root: &Path) -> Store;" }"##;
    let shapes = r##"shapes:
  - name: PackStore
    owner: global-pack-store
    file: crates/cli/src/pack.rs
    code: |-
      pub struct PackStore {
          pub packs_root: PathBuf,
      }"##;

    assert_eq!(broken(&plan(changes, shapes)), Vec::<String>::new());
    assert_eq!(
        broken(&plan(changes, "")),
        ["change-code-misses-a-name"],
        "without the shape, nothing shows `PackStore`"
    );
}

#[test]
fn a_refusal_names_what_the_code_leaves_out() {
    let changes = r##"      - { at: "crates/cli/src/commands/pack.rs::add/update/remove", what: "route each command", code: "pub async fn add(source: &str) -> Result<Outcome, CliError>;" }"##;
    let tasks: TasksFile = yunta_core::yaml::parse(&plan(changes, "")).unwrap();
    let said: Vec<String> = tasks
        .unexplained()
        .iter()
        .filter_map(|diagnostic| match &diagnostic.problem {
            yunta_core::diagnostic::Problem::Rule { code, detail }
                if code.to_string() == "change-code-misses-a-name" =>
            {
                Some(detail.clone())
            }
            _ => None,
        })
        .collect();

    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("names `update`, `remove`") && !said[0].contains("`add`,"),
        "{said:?}"
    );
}
