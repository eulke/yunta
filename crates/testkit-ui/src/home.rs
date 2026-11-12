//! The places the walk's recordings are made in: a directory in no repository, a
//! repository never set up and another the walk sets up, one set up with a workflow and
//! no run, a repository with runs, one finished and one parked on a gate, and three
//! repositories of one run each: parked on a gate for the walk to decide, on a round of
//! questions for it to answer, and on its plan's gate for the walk to open the application
//! on. Those three stand apart, since every run the field lists multiplies the places a
//! pointer and the picks can stand. Each stage that joins adds what its work reads: a
//! plan, a questions node, packs.

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use yunta_core::RunId;
use yunta_testkit::workflows::{ASKS, FINISHES, GREETS, HANDS_ONE_QUESTION_OVER};
use yunta_testkit::Bench;

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
