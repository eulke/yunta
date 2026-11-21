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
//! makes ([`DesignsPlaces`]). The design's machine answers for two adapters, so a `codex`
//! stands on their `PATH` beside the walk's `claude`; the walk's own places find the
//! `claude` alone. They are made the first time a test opens one: no walk opens any, and
//! the past runs of fragua alone take seconds to make.
//!
//! And the runs whose screens the walk opens on a scene of a run's screen, each as its
//! target's cut stands it ([`TARGET_CUTS`]): the design's run at the moments of
//! `DESIGNS_MOMENTS`, a cut a moment, since one store holds a run at one head; the small
//! runs of `SMALL_RUNS` in one cut, their ids their own, with the failure's run as its
//! command starts beside them, and the same run, its failure answered elsewhere, in a cut of
//! its own; a parent parked on its child in a cut of its own; and a run asking one question
//! picked from a list, made as the home's asking run is. A moment or a run joins where it stands at a scene no run before it
//! stands at: a scene it only tells apart otherwise, a round of choices where the home's
//! round is typed, is no reason, since every run's screen the walk opens multiplies by the
//! nodes it points, the rows it scrolls and the fields it stands over.

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use tokio::sync::OnceCell;
use yunta_api::Workspace;
use yunta_core::{OptionId, Responder, RunId};
use yunta_testkit::reference_pack::{self, ReferencePack};
use yunta_testkit::workflows::{ASKS, FINISHES, GREETS, HANDS_ONE_QUESTION_OVER};
use yunta_testkit::Bench;

use crate::cut::{Built, Cut, HomePlace, Shown, Stands, TARGET_CUTS, WITH_FRAGUA};
use crate::sandbox::{workspace, workspace_on, workspace_reaching};

/// The targets whose cuts stand the design's run at the moments the walk opens its screen
/// at, each a scene of a run's screen no other run of the home stands at: a loop at work,
/// what the run holds every task to among it; the scope asked, and before what it asks is
/// read, the pause read from the log alone; the findings; the suite's failure; and the run
/// finished, its receipt and its numbers, which pass a screen.
const DESIGNS_MOMENTS: [&str; 5] = ["board", "scope", "findings", "failure", "receipt"];

/// The targets whose cuts stand a small run at a scene of a run's screen the design's
/// moments do not stand at, each run at its target's seq: a session at work again after its
/// gate sent it back, what it hands over not yet handed; a round whose answers were refused;
/// a gate's card with no plan; a departure; the token cap; the loop cap; a decision whose
/// document cannot be read; a pause no option answers; a run that wakes alone; a pull
/// request; no room; work left half done; a run a person stopped; a stalled run; a question
/// asked at another terminal; a run created; a report; a failed, a promoted and a broken
/// run's end; and a node the mode leaves out.
const SMALL_RUNS: [&str; 21] = [
    "node-sent-back",
    "round-refused",
    "card",
    "departure",
    "budget",
    "loop-cap",
    "card-unread",
    "standstill",
    "wakes-alone",
    "pull-request",
    "no-room",
    "orphans",
    "stopped",
    "stalled",
    "asked-elsewhere",
    "created",
    "reported",
    "failed",
    "promoted",
    "broken",
    "node-skipped",
];

/// A command at work, which no target's cut stands at: the failure's run as its command
/// starts, its engine alive. It stands beside the small runs.
const COMMAND_AT_WORK: Shown = Shown::at(Built::Failure, 2).alive();

/// The failure's run parked on its failure, which [`answered_elsewhere`] answers.
const FAILURE_PARKED: Shown = Shown::at(Built::Failure, 4);

/// What the failure is answered with, and by whom: going on, as a lead would.
const ANSWERED: (&str, &str) = ("retry", "lead");

/// A node that asks one question picked from a list, the round of choices: its session
/// hands the question over and ends.
const HANDS_A_CHOICE_OVER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - id: train
                text: "Which release train carries it?"
                answer_type: choice
                values: ["this week", "next week"]
                required: true
    outcome: { type: completed, summary: asked }
"#;

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
    /// The places the design's starting screens stand in, made the first time one is
    /// opened ([`Home::workspace_at`]): no walk opens one.
    designs: OnceCell<DesignsPlaces>,
    /// The runs whose scenes the walk opens.
    pub scenes: Scenes,
}

/// The places the design's starting screens stand in, which only a target's test opens.
/// Not [`Home`]'s own places, which the walk's recordings are made in.
pub struct DesignsPlaces {
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

impl DesignsPlaces {
    /// Each place, made under `root`.
    async fn built(root: &Path) -> DesignsPlaces {
        DesignsPlaces {
            codex: crate::sandbox::codex_answering(root),
            reference_pack: ReferencePack::made(&root.join("upstream/yunta")),
            setting_up: designs_repository(root).await,
            with_fragua: designs_repository_set_up().await,
        }
    }
}

/// The runs whose scenes of a run's screen the walk opens, each in the cut its target's row
/// of [`TARGET_CUTS`] stands it at. Not [`Home`]'s places, which the walk's field and its
/// other screens stand in.
pub struct Scenes {
    /// The design's run at each moment of `DESIGNS_MOMENTS`, in a cut of its own.
    pub designs_moments: Vec<Cut>,
    /// The small runs of `SMALL_RUNS` and `COMMAND_AT_WORK`, in one cut.
    pub small_runs: Cut,
    /// A parent parked on its child, which its screen shows in its place: in a cut of its
    /// own, since a run's child is minted the id a promotion's successor is minted in the
    /// store of another small run.
    pub child: Cut,
    /// The failure's run with its failure answered elsewhere, its resume not yet on the
    /// log (`answered_elsewhere`): in a cut of its own, since the small runs' holds the
    /// same run at its command's start.
    pub answered: Cut,
    /// A repository whose run is parked on a round of one question picked from a list,
    /// which the walk answers: as the home's asking run, whose question is typed.
    pub choosing: Bench,
}

impl Scenes {
    /// Each run, cut and loaded.
    async fn built() -> Scenes {
        let mut designs_moments = Vec::new();
        for moment in DESIGNS_MOMENTS {
            designs_moments.push(Cut::of(&[shown_by(moment)]).await);
        }
        let shown: Vec<Shown> = SMALL_RUNS
            .map(shown_by)
            .into_iter()
            .chain([COMMAND_AT_WORK])
            .collect();
        let choosing = Bench::with_run_id("01M7WALKCH00S1NG0000CH00SE");
        choosing.run(ASKS, HANDS_A_CHOICE_OVER).await;
        let small_runs = Cut::of(&shown).await;
        let child = Cut::of(&[shown_by("child")]).await;
        let answered = answered_elsewhere().await;
        Scenes {
            designs_moments,
            small_runs,
            child,
            answered,
            choosing,
        }
    }

    /// Every cut, each with the runs it shows.
    pub fn cuts(&self) -> impl Iterator<Item = &Cut> {
        self.designs_moments
            .iter()
            .chain([&self.small_runs, &self.child, &self.answered])
    }
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
        let scenes = Scenes::built().await;
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
            designs: OnceCell::new(),
            scenes,
        }
    }

    /// Every run of the home a trail can name: the runs it made itself, and every run the
    /// store of each cut the walk opens a screen in holds, a child and a successor among
    /// them, read from the store rather than from what the application follows.
    pub fn runs(&self) -> Vec<RunId> {
        let made = [
            &self.worked.run_id,
            &self.parked,
            &self.deciding.run_id,
            &self.asking.run_id,
            &self.reviewing.run_id,
            &self.scenes.choosing.run_id,
        ];
        let stored = self.scenes.cuts().flat_map(|cut| {
            let project = cut.workspace().project();
            let project = project.expect("a cut's workspace names its store");
            yunta_testkit::heads(&project.storage_path).into_keys()
        });
        made.into_iter().cloned().chain(stored).collect()
    }

    /// The workspace the application opens in at `place`: its repository, the state kept
    /// beside it, the CLIs on its `PATH` and the git it hands its processes. A place the
    /// design's starting screens stand in is made the first time one is opened.
    pub async fn workspace_at(&self, place: HomePlace) -> Workspace {
        let root = self.root();
        let designs = || self.designs.get_or_init(|| DesignsPlaces::built(root));
        let machine = |designs: &DesignsPlaces| vec![self.clis.clone(), designs.codex.clone()];
        match place {
            HomePlace::Outside => workspace(&self.outside, root),
            HomePlace::SettingUp => {
                let designs = designs().await;
                let reached = designs.reference_pack.reached();
                workspace_reaching(&designs.setting_up, root, machine(designs), reached)
            }
            HomePlace::PackOutOfReach => {
                let designs = designs().await;
                let nowhere = reference_pack::out_of_reach(&root.join(NOWHERE));
                workspace_reaching(&designs.setting_up, root, machine(designs), nowhere)
            }
            HomePlace::SetUp => workspace(&self.set_up, root),
            HomePlace::WithFragua => {
                let designs = designs().await;
                let cut = &designs.with_fragua;
                Workspace {
                    env: workspace_on(&cut.repository(), cut.root(), machine(designs)).env,
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

/// The run the target `name`'s cut shows, as its row of [`TARGET_CUTS`] stands it once its
/// keys are pressed: where they take the lead and the run then goes on, as a promoted run
/// closes, at what it goes on to, since the walk takes the lead itself.
fn shown_by(name: &str) -> Shown {
    let target = TARGET_CUTS
        .iter()
        .find(|target| target.name == name)
        .unwrap_or_else(|| panic!("no target is named `{name}`"));
    match target.gone_on().unwrap_or(target.stands) {
        Stands::Run(shown) => shown,
        Stands::Field(_) | Stands::Home(_) => panic!("`{name}` shows no one run's screen"),
    }
}

/// The failure's run parked on its failure in a cut of its own, answered as another process
/// answers it ([`ANSWERED`]), and handed on to the engine the cut hands runs to, which takes
/// it up and writes nothing: its answer on the log, its resume not yet.
async fn answered_elsewhere() -> Cut {
    let cut = Cut::of(&[FAILURE_PARKED]).await;
    let project = cut.workspace().project();
    let project = project.expect("a cut's workspace names its project");
    let run = project.run(&cut.runs()[0]).await;
    let run = run.expect("the failure's run, in the cut");
    let (option, by) = ANSWERED;
    let by: Responder = by.parse().expect("a responder's name");
    let decided = run.decide(OptionId::from(option), Some(&by), None).await;
    decided.expect("the failure answered elsewhere");
    cut
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

/// A repository set up as the design's, the cut of [`WITH_FRAGUA`]: its origin on GitHub,
/// its git committing as a person.
async fn designs_repository_set_up() -> Cut {
    let with_fragua = Cut::of(WITH_FRAGUA).await;
    let repository = with_fragua.repository();
    yunta_testkit::git(&repository, &["remote", "add", "origin", ORIGIN]);
    committing_as_a_person(&repository);
    with_fragua
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
