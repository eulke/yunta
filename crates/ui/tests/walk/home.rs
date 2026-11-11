//! The places the walk's recordings are made in: a directory in no repository, a
//! repository never set up and another the walk sets up, one set up with a workflow and
//! no run, a repository with
//! runs, one finished and one parked on a gate, and two repositories of one run each, parked on a gate for the walk to decide
//! and on a round of questions for it to answer. Those two stand apart, since every run
//! the field lists multiplies the places a pointer and the picks can stand. Each stage that joins adds what its work reads: a plan, a questions node,
//! packs.

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use yunta_core::RunId;
use yunta_testkit::Bench;

/// One node that finishes.
const FINISHES: &str = "name: done\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n";

/// A gate nobody answers: its run parks on it.
const PARKS: &str = "name: gated\nnodes:\n  - { id: approve, kind: gate, assignee: lead, \
                     message: \"Go on?\", options: [go] }\n";

/// The config a repository set up for Yunta holds.
const SET_UP: &str = "defaults:\n  isolation: none\n";

/// A workflow that takes what was asked for as its request.
const GREETS: &str = "name: greet\ninputs:\n  ask: { type: string, request: true }\nnodes:\n  \
                      - { id: say, kind: bash, run: \"true\" }\n";

/// A mock fixture with no sessions: neither workflow opens one.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// A case of the greeting workflow, which finishes.
const GREETS_CASE: &str = "workflow: greet\ninputs: { ask: hello }\nfixture: fixtures/none.yaml\n\
                           expect: { final_state: finished }\n";

/// A node that asks: its session hands a round of questions over and ends.
const ASKS: &str = "name: asking\nnodes:\n  - id: ask\n    kind: prompt\n    runner: executor\n    \
                    prompt: \"Ask what has to be known.\"\n    artifacts:\n      produces: [questions]\n";

/// The session that hands one question over.
const HANDS_THEM_OVER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - { id: summary, text: "What changed?", answer_type: text, required: true }
    outcome: { type: completed, summary: asked }
"#;

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
}

impl Home {
    /// Every place, made, and the runs of the worked repository run.
    pub async fn built() -> Home {
        let root = tempfile::tempdir().expect("a temporary root");
        let outside = root.path().join("outside");
        std::fs::create_dir_all(&outside).expect("a directory outside any repository");
        let never_set_up = root.path().join("never-set-up");
        std::fs::create_dir_all(&never_set_up).expect("a repository's directory");
        yunta_testkit::init_repo(&never_set_up);
        let to_set_up = root.path().join("to-set-up");
        std::fs::create_dir_all(&to_set_up).expect("a repository's directory");
        yunta_testkit::init_repo(&to_set_up);
        let clis = crate::support::claude_answering(root.path());
        let packed = crate::support::with_a_pack_installed(&root.path().join("packed")).await;
        let (tools, _) = crate::support::pack_with_an_executor(root.path());
        let set_up = root.path().join("set-up");
        std::fs::create_dir_all(set_up.join(".yunta/workflows")).expect("a repository's catalog");
        yunta_testkit::init_repo(&set_up);
        std::fs::write(set_up.join(".yunta/config.yaml"), SET_UP).expect("its config");
        std::fs::write(set_up.join(".yunta/workflows/greet.yaml"), GREETS).expect("its workflow");
        std::fs::create_dir_all(set_up.join(".yunta/tests/fixtures")).expect("its cases");
        std::fs::write(set_up.join(".yunta/tests/fixtures/none.yaml"), NO_SESSIONS)
            .expect("its fixture");
        std::fs::write(set_up.join(".yunta/tests/greets.yaml"), GREETS_CASE).expect("its case");
        let worked = Bench::with_run_id("01M7WALKFN1SHED00000000000");
        worked.run(FINISHES, NO_SESSIONS).await;
        let parked_bench = worked.beside("01M7WALKPARKED000000000000");
        parked_bench.run(PARKS, NO_SESSIONS).await;
        let parked = parked_bench.run_id.clone();
        let deciding = Bench::with_run_id("01M7WALKDEC1DED00000000000");
        deciding.run(PARKS, NO_SESSIONS).await;
        let asking = Bench::with_run_id("01M7WALKASK1NG000000000000");
        asking.run(ASKS, HANDS_THEM_OVER).await;
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
pub fn root_of(bench: &Bench) -> &Path {
    bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root")
}
