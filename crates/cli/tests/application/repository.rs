//! The repository's stages in the application, each reached as a person who does not know
//! its key reaches it, from the palette (`/`), and leaving what its command leaves: a
//! repository never set up opens on its setup, which writes what `yunta init` writes; the
//! catalog checks a workflow, writes a new one as `yunta new` writes it and runs the
//! cases; the packs are listed and audited; a document's shape is shown as `yunta schema`
//! prints it.

use std::path::Path;

use yunta_testkit::workflows::FINISHES;
use yunta_testkit::{git, stderr, stdout, yunta_at, yunta_in, yunta_on_terminal, Checkout};

use super::{given_back, opened, taken_from_the_palette};

/// What a setup says once it is written.
const SET_UP: &str = "for this repo's CLAUDE.md";

/// A file for a project never set up to hold, so it has something to commit.
const PROJECT: &str = "a project\n";

/// `checkout`'s config, as setting it up wrote it.
fn config(repo: &Path) -> String {
    std::fs::read_to_string(repo.join(".yunta/config.yaml")).expect("a config written")
}

/// A repository never set up opens on its setup; escaping every question writes what
/// `yunta init` writes unasked, beside it.
#[test]
fn a_repository_never_set_up_opens_on_its_setup_which_writes_what_init_writes() {
    let never = Checkout::new().file("README.md", PROJECT).committed();
    let mut terminal = yunta_on_terminal!(&never.repo, &never.home, &[]);
    terminal.wait_on_screen("project name", "it never opened on the setup");
    // As many as any project asks: once it is written, escape asks nothing more.
    for _ in 0..24 {
        if terminal.screen().holds(SET_UP) {
            break;
        }
        terminal.keys("\u{1b}");
    }
    terminal.wait_on_screen(SET_UP, "the setup was never written");
    let twin = Checkout::new().file("README.md", PROJECT).committed();
    let init = yunta_at!(&twin, &["init"]);
    assert!(init.status.success(), "{}", stderr(&init));
    assert_eq!(config(&never.repo), config(&twin.repo));
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// The catalog, from the palette: a workflow checked, a new one written as `yunta new`
/// writes it, and the repository's cases run.
#[test]
fn the_catalog_checks_writes_a_new_workflow_as_new_does_and_runs_the_cases() {
    let case = "workflow: quick\nfixture: fixtures/none.yaml\nexpect: { final_state: finished }\n";
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("done", FINISHES)
        .file(
            ".yunta/workflows/quick.yaml",
            "name: quick\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n",
        )
        .file(".yunta/tests/quick.yaml", case)
        .file(
            ".yunta/tests/fixtures/none.yaml",
            "dialect: claude-code\nsessions: []\n",
        )
        .committed();
    let ran = yunta_at!(&checkout, &["run", "done.yaml"]);
    assert!(ran.status.success(), "{}", stderr(&ran));
    let mut terminal = opened(&checkout);
    taken_from_the_palette(&mut terminal, "yunta list");
    terminal.wait_on_screen("quick", "the catalog never listed the workflow");
    terminal.keys("c");
    terminal.wait_on_screen("can run here", "the workflow was never checked");
    terminal.keys("\u{1b}");
    // The catalog back before the next key: an escape a letter follows at once reads as
    // that letter with Alt.
    terminal.wait_on_screen("checks it", "the catalog never came back");
    terminal.keys("n");
    terminal.wait_on_screen(
        "What is the new workflow called?",
        "a new workflow was never asked",
    );
    terminal.keys("fresh");
    terminal.keys("\r");
    // The first shape, `one-node`, as `yunta new` picks off a terminal.
    terminal.keys("\r");
    terminal.wait_on_screen("fresh.yaml", "the new workflow was never written");
    let twin = Checkout::new().working_in_place().committed();
    let new = yunta_at!(&twin, &["new", "fresh"]);
    assert!(new.status.success(), "{}", stderr(&new));
    let written = |repo: &Path| {
        std::fs::read_to_string(repo.join(".yunta/workflows/fresh.yaml")).expect("written")
    };
    assert_eq!(written(&checkout.repo), written(&twin.repo));
    terminal.keys("\r");
    terminal.keys("t");
    terminal.wait_on_screen("1 case, 0 failed", "the cases never ran");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// The packs, from the catalog the palette opens: listed, and one audited.
#[tokio::test]
async fn the_packs_are_listed_and_one_is_audited() {
    let root = tempfile::tempdir().unwrap();
    let repo = yunta_testkit_ui::sandbox::with_a_pack_installed(root.path()).await;
    std::fs::write(repo.join("done.yaml"), FINISHES).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "a run to list"]);
    let ran = yunta_in!(&repo, root.path(), &["run", "done.yaml"]);
    assert!(ran.status.success(), "{}", stderr(&ran));
    let mut terminal = yunta_on_terminal!(&repo, root.path(), &[]);
    terminal.wait_on_screen("runs of this repository", "it never opened on the field");
    taken_from_the_palette(&mut terminal, "yunta list");
    terminal.keys("p");
    terminal.wait_on_screen("acme/demo", "the packs were never listed");
    terminal.keys("a");
    terminal.wait_on_screen("demo-says", "the pack was never audited");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// A document's shape, from the palette: the shape `yunta schema` prints.
#[test]
fn a_documents_shape_is_shown_as_schema_prints_it() {
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("done", FINISHES)
        .committed();
    let ran = yunta_at!(&checkout, &["run", "done.yaml"]);
    assert!(ran.status.success(), "{}", stderr(&ran));
    let mut terminal = opened(&checkout);
    taken_from_the_palette(&mut terminal, "yunta schema");
    terminal.wait_on_screen("Which document's shape?", "the shapes were never asked");
    terminal.keys("tasks");
    terminal.keys("\r");
    let printed = stdout(&yunta_at!(&checkout, &["schema", "tasks"]));
    // `schema` prints what the shape says as YAML's comments; the application says it as
    // text.
    let first = printed
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .expect("what `yunta schema tasks` says of the shape")
        .to_string();
    terminal.wait_on_screen(&first, "the shape was never shown as `schema` prints it");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}
