//! The repository's stages in the application, each reached as a person who does not know
//! its key reaches it, from the palette (`/`), and leaving what its command leaves: a
//! repository never set up opens on its setup, which, its pack's audit read and its install
//! confirmed, leaves what `yunta pack add --yes` and `yunta init -i` leave with the same
//! answers; the catalog checks a workflow, writes a new one as `yunta new` writes it and runs
//! the cases; the packs are listed and audited; a document's shape is shown as `yunta schema`
//! prints it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use yunta_api::SetupPack;
use yunta_core::step::NextStep;
use yunta_render::screen::WORKFLOWS_YOU_CAN_RUN;
use yunta_testkit::reference_pack::ReferencePack;
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::{
    git, stderr, stdout, wait_until, yunta_at, yunta_in, yunta_on_terminal, Checkout, Terminal,
};

use super::{given_back, opened, taken_from_the_palette};

/// A file for a project never set up to hold, so it has something to commit.
const PROJECT: &str = "a project\n";

/// `checkout`'s config, as setting it up wrote it.
fn config(repo: &Path) -> String {
    std::fs::read_to_string(repo.join(".yunta/config.yaml")).expect("a config written")
}

/// A `PATH` under `root` on which the one adapter CLI is the scripted `claude`, so a
/// runner's questions are asked whatever CLIs the machine running the suite has.
fn only_claude(root: &Path) -> String {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).expect("a directory for the CLI");
    std::os::unix::fs::symlink(yunta_testkit_core::stubs::claude_code(), bin.join("claude"))
        .expect("the scripted CLI linked");
    format!("{}:/usr/bin:/bin", bin.display())
}

/// The environment a terminal is opened with: `path` for its `PATH`, then `git`, what points
/// its git where a test points it.
fn on_the_path<'a>(path: &'a str, git: &'a [(String, String)]) -> Vec<(&'a str, &'a str)> {
    let git = git
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()));
    [("PATH", path)].into_iter().chain(git).collect()
}

/// The roles fragua proposes, in the order a setup and `init -i` ask them.
const FRAGUA_ROLES: [&str; 5] = [
    "planner",
    "executor",
    "reviewer",
    "reviewer-alt",
    "mechanical",
];

/// The role whose model is typed, the one over the row that writes.
const TYPED_FOR: &str = "mechanical";

/// The model typed for it.
const MODEL: &str = "opus";

/// Every file under `dir`, by its path under it, with its bytes.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut walked = vec![dir.to_path_buf()];
    while let Some(at) = walked.pop() {
        for entry in std::fs::read_dir(&at).expect("a directory, read") {
            let path = entry.expect("an entry").path();
            match path.is_dir() {
                true => walked.push(path),
                false => {
                    let under = path
                        .strip_prefix(dir)
                        .expect("under the tree")
                        .to_path_buf();
                    files.insert(under, std::fs::read(&path).expect("a file, read"));
                }
            }
        }
    }
    files
}

/// What setting `repo` up left: its config, its lock and fragua's vendored tree.
fn left_by_the_setup(repo: &Path) -> (String, String, BTreeMap<PathBuf, Vec<u8>>) {
    let lock = std::fs::read_to_string(repo.join(".yunta/yunta.lock")).expect("a lock written");
    let fragua = tree(&repo.join(".yunta/packs/yunta/fragua"));
    (config(repo), lock, fragua)
}

/// Waits until `terminal` has drawn `needle` `times` times, failing with `what`: a prompt
/// asked again for each role is told by how many times it was drawn, a line answered drawn
/// again as it is echoed.
fn drawn_times(terminal: &Terminal, needle: &str, times: usize, what: &str) {
    wait_until(
        || terminal.drawn().matches(needle).count() >= times,
        || format!("{what}\ndrawn so far:\n{}", terminal.drawn()),
    );
}

/// `terminal`, open on the setup of a repository whose git reaches the local fragua: the model
/// of [`TYPED_FOR`] typed in place on its row, then `enter` on the row that writes, on fragua's
/// audit and on its confirmation, which installs fragua and writes the setup.
fn set_up_with_fragua(terminal: &mut Terminal) {
    terminal.wait_on_screen("what this repository is", "it never opened on the setup");
    terminal.wait_on_screen("to install as a pack", "fragua was never fetched");
    terminal.keys("k");
    terminal.wait_on_screen("enter change it", "the pointer never stood on the role");
    terminal.keys("\r");
    terminal.wait_on_screen("←→ adapter", "the role never opened in place");
    terminal.keys("\r");
    terminal.wait_on_screen("type the model", "the role's model was never asked");
    terminal.keys(MODEL);
    terminal.keys("\r");
    let candidate = format!("claude-code/{MODEL}");
    terminal.wait_on_screen(&candidate, "the model was never taken into the role");
    terminal.keys("j");
    terminal.wait_on_screen("enter write it", "the pointer never came back to the write");
    terminal.keys("\r");
    terminal.wait_on_screen("esc back", "fragua's audit never stood");
    terminal.keys("\r");
    terminal.wait_on_screen("esc write without it", "fragua's confirmation never stood");
    terminal.keys("\r");
    let wrote = "Wrote .yunta/config.yaml and installed yunta/fragua";
    terminal.wait_on_screen(wrote, "the setup was never written with fragua");
}

/// `twin` set up at a shell as the application set up its own: fragua added with `--yes` from
/// the source a setup fetches it from, its git as `git` points it, then `init -i` at a terminal
/// with `vars`, every question left but the model of [`TYPED_FOR`], typed.
fn set_up_at_a_shell(twin: &Checkout, git: &[(String, String)], vars: &[(&str, &str)]) {
    let source = SetupPack::of_this_binary();
    let adding = ["pack", "add", source.source(), "--yes"];
    let added = yunta_in!(&twin.repo, &twin.home, &adding, git);
    assert!(added.status.success(), "{}", stderr(&added));
    let mut init = yunta_on_terminal!(&twin.repo, &twin.home, &["init", "-i"], vars);
    for (asked, what) in [
        ("name (enter keeps", "init never asked the project's name"),
        ("base (enter keeps", "init never asked the base branch"),
    ] {
        init.wait_for(asked, what);
        init.keys("\u{1b}");
    }
    for (asked, role) in FRAGUA_ROLES.iter().enumerate() {
        let role_asked = format!("the adapter `{role}` runs on");
        init.wait_for(&role_asked, "init never asked the role");
        // A list answered is not drawn again; a model's line is, echoed once answered.
        let offered = "init never offered the adapter";
        drawn_times(&init, "▸ claude-code", asked + 1, offered);
        init.keys("\r");
        let model_asked = "init never asked the model";
        drawn_times(&init, "model (enter with none", 2 * asked + 1, model_asked);
        match *role == TYPED_FOR {
            true => init.keys(&format!("{MODEL}\r")),
            false => init.keys("\r"),
        }
    }
    let drawn = init.ended();
    assert!(init.ran_to_the_end(), "{drawn}");
}

/// A repository never set up opens on its setup, its git pointing the Yunta repository's URL at
/// the local fragua: one role's model typed in place, then `enter` on the row that writes, on
/// fragua's audit and on its confirmation, installs fragua and writes the setup, leaving the
/// config, the lock and fragua's tree a twin leaves after `yunta pack add <source> --yes` and
/// `yunta init -i` with the same answers; `enter` on what was written goes on to the first
/// request, and `esc` there to the field, which holds no run yet.
#[test]
fn a_repository_never_set_up_opens_on_its_setup_and_writes_what_init_writes_with_fragua_installed()
{
    let bins = tempfile::tempdir().expect("a temporary root");
    let path = only_claude(bins.path());
    let reference = ReferencePack::made(&bins.path().join("upstream/yunta"));
    let git = reference.reached();
    let vars = on_the_path(&path, &git);
    let never = Checkout::new().file("README.md", PROJECT).committed();
    let mut terminal = yunta_on_terminal!(&never.repo, &never.home, &[], &vars);
    set_up_with_fragua(&mut terminal);
    let twin = Checkout::new().file("README.md", PROJECT).committed();
    set_up_at_a_shell(&twin, &git, &vars);
    let (set_up, twins) = (
        left_by_the_setup(&never.repo),
        left_by_the_setup(&twin.repo),
    );
    let typed = format!("    - adapter: claude-code\n      model: {MODEL}\n");
    assert!(set_up.0.contains(&typed), "{}", set_up.0);
    assert_eq!(set_up.0, twins.0, "the config");
    assert_eq!(set_up.1, twins.1, "the lock");
    assert!(set_up.2 == twins.2, "fragua's tree");
    assert!(!set_up.2.is_empty(), "fragua is vendored");
    terminal.keys("\r");
    terminal.wait_on_screen("what you want to build", "the first request never followed");
    terminal.keys("\u{1b}");
    terminal.wait_on_screen(super::THE_FIELD, "`esc` never went to the field");
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
    taken_from_the_palette(&mut terminal, &NextStep::List);
    terminal.wait_on_screen("quick", "the catalog never listed the workflow");
    terminal.keys("c");
    terminal.wait_on_screen("can run here", "the workflow was never checked");
    terminal.keys("\u{1b}");
    // The catalog back before the next key: an escape a letter follows at once reads as
    // that letter with Alt.
    terminal.wait_on_screen(WORKFLOWS_YOU_CAN_RUN, "the catalog never came back");
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
    terminal.wait_on_screen(super::THE_FIELD, "`q` on the cases never went to the field");
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
    terminal.wait_on_screen(super::THE_FIELD, "it never opened on the field");
    taken_from_the_palette(&mut terminal, &NextStep::List);
    terminal.keys("p");
    terminal.wait_on_screen("acme/demo", "the packs were never listed");
    terminal.keys("a");
    terminal.wait_on_screen("demo-says", "the pack was never audited");
    terminal.keys("q");
    terminal.wait_on_screen(super::THE_FIELD, "`q` on the audit never went to the field");
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
    taken_from_the_palette(&mut terminal, &NextStep::Schemas);
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
    terminal.wait_on_screen(super::THE_FIELD, "`q` on the shape never went to the field");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}
