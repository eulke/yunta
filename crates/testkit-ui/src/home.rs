//! The places the walk's recordings are made in: a directory in no repository, a
//! repository never set up and another the walk sets up, one set up with a workflow and
//! no run, a repository with runs, one finished and one parked on a gate, and three
//! repositories of one run each: parked on a gate for the walk to decide, on a round of
//! questions for it to answer, and on its plan's gate for the walk to open the application
//! on. Those three stand apart, since every run the field lists multiplies the places a
//! pointer and the picks can stand. Each stage that joins adds what its work reads: a
//! plan, a questions node, packs.
//!
//! And the places the design's starting screens stand in, which a target names
//! ([`HomePlace`]) and [`Home::workspace_at`] opens: the design's repository before its
//! setup, with the pack a setup installs in reach, in a local repository git reads in the
//! Yunta repository's place ([`ReferencePack`]), or out of reach; and a repository set up
//! as the design's, fragua installed and its past runs, which a cut of [`WITH_FRAGUA`]
//! makes. The design's machine answers for two adapters, so a `codex` stands on their
//! `PATH` beside the walk's `claude`; the walk's own places find the `claude` alone.

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use yunta_api::Workspace;
use yunta_core::RunId;
use yunta_testkit::reference_pack::{self, ReferencePack};
use yunta_testkit::workflows::{ASKS, FINISHES, GREETS, HANDS_ONE_QUESTION_OVER};
use yunta_testkit::Bench;

use crate::cut::{Cut, HomePlace, WITH_FRAGUA};
use crate::sandbox::{workspace, workspace_on, workspace_reaching};

/// A gate nobody answers: its run parks on it.
const PARKS: &str = "name: gated\nnodes:\n  - { id: approve, kind: gate, assignee: lead, \
                     message: \"Go on?\", options: [go] }\n";

/// The config a repository set up for Yunta holds.
const SET_UP: &str = "defaults:\n  isolation: none\n";

/// A mock fixture with no sessions: neither workflow opens one.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// A case of the greeting workflow, which finishes.
const GREETS_CASE: &str = "workflow: greet\ninputs: { ask: hello }\nfixture: fixtures/none.yaml\n\
                           expect: { final_state: finished }\n";

/// The design's repository's manifest: a Rust workspace, which a setup reads its
/// commands off.
const CARGO_WORKSPACE: &str = "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\"]\n";

/// The repository on GitHub the design's repository's origin is, as git names it over
/// ssh, which no pointing of the Yunta repository's https URL rewrites.
const ORIGIN: &str = "git@github.com:eulke/yunta.git";

/// Where, under the home's root, no repository stands: what the Yunta repository's URL
/// points at where the pack a setup installs is out of reach.
const NOWHERE: &str = "nowhere";

/// Who git commits as in the design's repository, before its setup and set up: a person,
/// by a name and an address, as the design's machine says who commits.
const COMMITTER: (&str, &str) = ("Marta Ruiz", "marta.ruiz@example.com");

/// The scratch places.
pub struct Home {
    root: TempDir,
    /// A directory in no git repository.
    pub outside: PathBuf,
    /// A git repository Yunta was never set up in.
    pub never_set_up: PathBuf,
    /// A git repository Yunta was never set up in, which the walk's recordings set up.
    pub to_set_up: PathBuf,
    /// A git repository set up for Yunta with the pack `acme/demo` installed.
    pub packed: PathBuf,
    /// A local repository holding the pack `acme/tools`, which ships an executor.
    pub tools: PathBuf,
    /// Where the CLIs the walk stands in for are, for the `PATH` of a setup: a
    /// `claude` that answers its probe, so each proposed runner's adapter and model
    /// are asked.
    pub clis: PathBuf,
    /// A git repository set up for Yunta, with one workflow, a case of it, and no run.
    pub set_up: PathBuf,
    /// A repository with runs, its state kept beside it.
    pub worked: Bench,
    /// The run of `worked` parked on its gate.
    pub parked: RunId,
    /// A repository whose run is parked on a gate, which the walk decides.
    pub deciding: Bench,
    /// A repository whose run is parked on a round of questions, which the walk answers.
    pub asking: Bench,
    /// A repository whose run is parked on its plan's gate, which the walk opens the
    /// application on with `review`. Its id ends its own handle, `REV1EW`, so the screens
    /// it reaches are told apart.
    pub reviewing: Bench,
    /// Where a `codex` that answers its probe is: beside `clis` on the `PATH` of a place
    /// the design's screens stand in, the second adapter the design's machine answers for.
    pub codex: PathBuf,
    /// The Yunta repository a setup fetches the pack it installs from, its `packs/fragua`
    /// under the binary's version tag.
    pub reference_pack: ReferencePack,
    /// The design's repository before its setup: a Rust workspace named `yunta`, its
    /// origin on GitHub, its git committing as a person, never set up.
    pub setting_up: PathBuf,
    /// A repository set up as the design's: `yunta/fragua` installed, its own `lint-fix`
    /// and `run-tasks`, three finished runs of fragua, a run that needs the person and one
    /// stalled, its origin on GitHub, its git committing as a person.
    pub with_fragua: Cut,
}

impl Home {
    /// Every place, made, and the runs of the worked repository run.
    pub async fn built() -> Home {
        let root = tempfile::tempdir().expect("a temporary root");
        let outside = root.path().join("outside");
        tokio::fs::create_dir_all(&outside)
            .await
            .expect("a directory outside any repository");
        let never_set_up = repository(root.path(), "never-set-up").await;
        let to_set_up = repository(root.path(), "to-set-up").await;
        let clis = crate::sandbox::claude_answering(root.path());
        let packed = crate::sandbox::with_a_pack_installed(&root.path().join("packed")).await;
        let (tools, _) = crate::sandbox::pack_with_an_executor(root.path());
        let set_up = set_up_with_a_case(root.path()).await;
        let worked = Bench::with_run_id("01M7WALKFN1SHED00000000000");
        worked.run(FINISHES, NO_SESSIONS).await;
        let parked_bench = worked.beside("01M7WALKPARKED000000000000");
        parked_bench.run(PARKS, NO_SESSIONS).await;
        let parked = parked_bench.run_id.clone();
        let deciding = Bench::with_run_id("01M7WALKDEC1DED00000000000");
        deciding.run(PARKS, NO_SESSIONS).await;
        let asking = Bench::with_run_id("01M7WALKASK1NG000000000000");
        asking.run(ASKS, HANDS_ONE_QUESTION_OVER).await;
        let reviewing = Bench::with_run_id("01M7WALKPLANNED00000REV1EW");
        crate::planned::parked_on_its_plan(&reviewing).await;
        let codex = crate::sandbox::codex_answering(root.path());
        let reference_pack = ReferencePack::made(&root.path().join("upstream/yunta"));
        let setting_up = designs_repository(root.path()).await;
        let with_fragua = Cut::of(WITH_FRAGUA).await;
        yunta_testkit::git(
            &with_fragua.repository(),
            &["remote", "add", "origin", ORIGIN],
        );
        committing_as_a_person(&with_fragua.repository());
        Home {
            root,
            outside,
            never_set_up,
            to_set_up,
            packed,
            tools,
            clis,
            set_up,
            worked,
            parked,
            deciding,
            asking,
            reviewing,
            codex,
            reference_pack,
            setting_up,
            with_fragua,
        }
    }

    /// The workspace the application opens in at `place`: its repository, the state kept
    /// beside it, the CLIs on its `PATH` and the git it hands its processes.
    pub fn workspace_at(&self, place: HomePlace) -> Workspace {
        let root = self.root();
        let machine = || vec![self.clis.clone(), self.codex.clone()];
        match place {
            HomePlace::Outside => workspace(&self.outside, root),
            HomePlace::SettingUp => {
                let reached = self.reference_pack.reached();
                workspace_reaching(&self.setting_up, root, machine(), reached)
            }
            HomePlace::PackOutOfReach => {
                let nowhere = reference_pack::out_of_reach(&root.join(NOWHERE));
                workspace_reaching(&self.setting_up, root, machine(), nowhere)
            }
            HomePlace::SetUp => workspace(&self.set_up, root),
            HomePlace::WithFragua => {
                let cut = &self.with_fragua;
                Workspace {
                    env: workspace_on(&cut.repository(), cut.root(), machine()).env,
                    ..cut.workspace()
                }
            }
            HomePlace::Packed => workspace(&self.packed, root),
            HomePlace::Worked => workspace(&self.worked.worktree, self.worked_root()),
        }
    }

    /// The state root the places outside the worked repository keep their state in.
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// The state root of the worked repository: where its runs and log are.
    pub fn worked_root(&self) -> &Path {
        root_of(&self.worked)
    }
}

/// The state root of `bench`'s repository: where its runs and log are.
pub(crate) fn root_of(bench: &Bench) -> &Path {
    bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root")
}

/// A git repository named `name` under `root`, which Yunta was never set up in.
async fn repository(root: &Path, name: &str) -> PathBuf {
    let repo = root.join(name);
    tokio::fs::create_dir_all(&repo)
        .await
        .expect("a repository's directory");
    yunta_testkit::init_repo(&repo);
    repo
}

/// `repo`'s git committing as [`COMMITTER`].
fn committing_as_a_person(repo: &Path) {
    let (name, address) = COMMITTER;
    yunta_testkit::git(repo, &["config", "user.name", name]);
    yunta_testkit::git(repo, &["config", "user.email", address]);
}

/// The design's repository before its setup, under `root`: a Rust workspace named
/// `yunta`, committed by a person, its origin on GitHub.
async fn designs_repository(root: &Path) -> PathBuf {
    let repo = repository(root, "yunta").await;
    committing_as_a_person(&repo);
    tokio::fs::write(repo.join("Cargo.toml"), CARGO_WORKSPACE)
        .await
        .expect("its manifest");
    yunta_testkit::git(&repo, &["add", "."]);
    yunta_testkit::git(&repo, &["commit", "-q", "-m", "workspace"]);
    yunta_testkit::git(&repo, &["remote", "add", "origin", ORIGIN]);
    repo
}

/// A git repository under `root` set up for Yunta, with one workflow, a case of it, and
/// no run.
async fn set_up_with_a_case(root: &Path) -> PathBuf {
    let set_up = root.join("set-up");
    tokio::fs::create_dir_all(set_up.join(".yunta/workflows"))
        .await
        .expect("a repository's catalog");
    yunta_testkit::init_repo(&set_up);
    tokio::fs::write(set_up.join(".yunta/config.yaml"), SET_UP)
        .await
        .expect("its config");
    tokio::fs::write(set_up.join(".yunta/workflows/greet.yaml"), GREETS)
        .await
        .expect("its workflow");
    tokio::fs::create_dir_all(set_up.join(".yunta/tests/fixtures"))
        .await
        .expect("its cases");
    tokio::fs::write(set_up.join(".yunta/tests/fixtures/none.yaml"), NO_SESSIONS)
        .await
        .expect("its fixture");
    tokio::fs::write(set_up.join(".yunta/tests/greets.yaml"), GREETS_CASE)
        .await
        .expect("its case");
    set_up
}
