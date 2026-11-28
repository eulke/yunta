//! See [`super`]. The places the design's starting screens stand in, which a target names
//! ([`HomePlace`]): the design's repository before its setup, a Rust workspace whose origin is
//! on GitHub and whose git commits as a person; and the same repository set up with fragua and
//! its past runs, a cut of [`WITH_FRAGUA`], its forge out of reach for want of a token and its
//! checkouts keeping of the disk what the design's kept. Made with the whole home
//! ([`DesignsPlaces`]), or alone for a screen held to its target ([`HomePlace::made_alone`]),
//! as every place of the home is; a place made alone says what it made afresh on the machine,
//! which a target's frames hold a placeholder for ([`Figure`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_api::Workspace;
use yunta_core::CommitSha;

use yunta_testkit::reference_pack::{self, ReferencePack};
use yunta_testkit::Bench;

use super::{
    repository, root_of, set_up_with_a_case, worked, CHECKOUTS_KEEP, NOWHERE, OUTSIDE, UPSTREAM,
};
use crate::cut::{Cut, HomePlace, WITH_FRAGUA};
use crate::sandbox::{workspace, workspace_on, workspace_on_git_alone, workspace_reaching};

/// The design's repository's manifest: a Rust workspace, which a setup reads its
/// commands off.
const CARGO_WORKSPACE: &str = "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\"]\n";

/// The repository on GitHub the design's repository's origin is, as git names it over
/// ssh, which no pointing of the Yunta repository's https URL rewrites.
const ORIGIN: &str = "git@github.com:eulke/yunta.git";

/// Who git commits as in the design's repository, before its setup and set up: a person,
/// by a name and an address, as the design's machine says who commits.
const COMMITTER: (&str, &str) = ("Marta Ruiz", "marta.ruiz@example.com");

/// The places the design's starting screens stand in, which only a target's test opens.
/// Not [`Home`](super::Home)'s own places, which the walk's recordings are made in.
pub struct DesignsPlaces {
    /// Where a `codex` that answers its probe is: beside `clis` on the `PATH` of a place
    /// the design's screens stand in, the second adapter the design's machine answers for.
    pub codex: PathBuf,
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
    pub(super) async fn built(root: &Path) -> DesignsPlaces {
        DesignsPlaces {
            codex: crate::sandbox::codex_answering(root),
            setting_up: designs_repository(root).await,
            with_fragua: designs_repository_set_up().await,
        }
    }
}

/// A place of the home made alone, which a screen held to its target opens in: the workspace
/// the application opens in there, where the place keeps its state, the figures it made afresh
/// on this machine, and what its runs stand in where it holds runs, kept while the place is.
pub struct MadeAlone {
    pub workspace: Workspace,
    /// The directory the place keeps its state under: the root it was made under, or its
    /// runs' own.
    pub root: PathBuf,
    /// What the place made afresh on this machine that a target's frames hold a placeholder
    /// for: none but where it installed a pack.
    pub figures: Vec<Figure>,
    _runs: Option<Runs>,
}

/// A figure a place makes afresh on each machine, which a target's frames hold a placeholder
/// for: the commit a pack is locked at, which git stamps with the time it was made. Not a
/// fixed fact of the place, which a frame says as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Figure {
    /// What a target's frames hold in the figure's place.
    pub placeholder: &'static str,
    /// The figure, as a screen says it.
    pub said: String,
}

/// What a target's frames hold in place of the commit a pack is locked at.
const COMMIT: &str = "{commit}";

/// What a place made alone keeps its runs in while it is: the cut they stand in, or the bench
/// they were run on, each kept for its tree alone; a bench on the heap, since it is far the
/// larger.
enum Runs {
    Cut { _in: Cut },
    Bench { _on: Box<Bench> },
}

impl HomePlace {
    /// The place made alone under `root`: what a target that opens there reads, built as the
    /// whole home builds it, with nothing else of the home beside it. The repository set up
    /// with no run is made under `root`, which keeps its state; so is the design's repository
    /// before its setup, its machine's CLIs and the Yunta repository its git reads, or the
    /// directory no repository stands in where the pack a setup installs is out of reach; the
    /// design's repository set up with fragua stands in a cut of its own, which keeps its
    /// runs, its machine's CLIs under `root`; the repository set up with runs, on a bench of
    /// its own, which keeps its state; the repository with a pack installed under `root`,
    /// beside the pack's own repository, with the commit it is locked at among its figures;
    /// and a directory in no repository under `root`, which keeps its state.
    pub async fn made_alone(self, root: &Path) -> MadeAlone {
        match self {
            HomePlace::SetUp => MadeAlone {
                workspace: workspace(&set_up_with_a_case(root).await, root),
                root: root.to_path_buf(),
                figures: Vec::new(),
                _runs: None,
            },
            HomePlace::WithFragua => {
                let cut = designs_repository_set_up().await;
                let machine = vec![
                    crate::sandbox::claude_answering(root),
                    crate::sandbox::codex_answering(root),
                ];
                MadeAlone {
                    workspace: with_fragua(&cut, machine),
                    root: cut.root().to_path_buf(),
                    figures: Vec::new(),
                    _runs: Some(Runs::Cut { _in: cut }),
                }
            }
            HomePlace::SettingUp | HomePlace::PackOutOfReach => {
                let machine = vec![
                    crate::sandbox::claude_answering(root),
                    crate::sandbox::codex_answering(root),
                ];
                let git = match self {
                    HomePlace::SettingUp => ReferencePack::made(&root.join(UPSTREAM)).reached(),
                    _ => reference_pack::out_of_reach(&root.join(NOWHERE)),
                };
                let repository = designs_repository(root).await;
                MadeAlone {
                    workspace: workspace_reaching(&repository, root, machine, git),
                    root: root.to_path_buf(),
                    figures: Vec::new(),
                    _runs: None,
                }
            }
            HomePlace::Worked => worked_alone().await,
            HomePlace::Packed => packed_alone(root).await,
            HomePlace::Outside => outside_alone(root).await,
        }
    }
}

/// A directory in no git repository made alone under `root`, which keeps its state: the
/// workspace the application opens in there, outside any repository.
async fn outside_alone(root: &Path) -> MadeAlone {
    let outside = root.join(OUTSIDE);
    tokio::fs::create_dir_all(&outside)
        .await
        .expect("a directory outside any repository");
    MadeAlone {
        workspace: workspace(&outside, root),
        root: root.to_path_buf(),
        figures: Vec::new(),
        _runs: None,
    }
}

/// The repository set up with a pack installed made alone under `root`, which keeps its state
/// and the pack's own repository: the workspace the application opens in there, and the commit
/// the pack is locked at as a screen says it, which git stamped with the time it was made.
async fn packed_alone(root: &Path) -> MadeAlone {
    let repository = crate::sandbox::with_a_pack_installed(root).await;
    let commit = crate::sandbox::commit_of(&crate::sandbox::pack_installed_from(root));
    let commit: CommitSha = commit.parse().expect("the commit git names");
    MadeAlone {
        workspace: workspace(&repository, root),
        root: root.to_path_buf(),
        figures: vec![Figure {
            placeholder: COMMIT,
            said: commit.abbreviated().to_string(),
        }],
        _runs: None,
    }
}

/// The repository set up with runs made alone, on a bench of its own, which keeps its state:
/// the workspace the application opens in there, git alone on its `PATH`.
async fn worked_alone() -> MadeAlone {
    let (worked, _) = worked().await;
    let kept = root_of(&worked).to_path_buf();
    MadeAlone {
        workspace: workspace_on_git_alone(&worked.worktree, &kept),
        root: kept,
        figures: Vec::new(),
        _runs: Some(Runs::Bench {
            _on: Box::new(worked),
        }),
    }
}

/// The workspace in the design's repository set up with fragua, `cut`, reading the state its
/// runs stand in, with the CLIs of `machine` on its `PATH`. It reaches the forge its config
/// declares as a workspace's own source does, through the token its environment names, which
/// is not set: the design's forge, out of reach.
pub(super) fn with_fragua(cut: &Cut, machine: Vec<PathBuf>) -> Workspace {
    Workspace {
        env: workspace_on(&cut.repository(), cut.root(), machine).env,
        forges: Arc::new(yunta_api::DeclaredForge),
        ..cut.workspace()
    }
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
/// its git committing as a person, its checkouts keeping of the disk what the design's kept.
async fn designs_repository_set_up() -> Cut {
    let with_fragua = Cut::of(WITH_FRAGUA).await;
    let repository = with_fragua.repository();
    yunta_testkit::git(&repository, &["remote", "add", "origin", ORIGIN]);
    committing_as_a_person(&repository);
    let config = repository.join(".yunta/config.yaml");
    let mut kept = tokio::fs::read_to_string(&config)
        .await
        .expect("the design's repository's config");
    kept.push_str(CHECKOUTS_KEEP);
    tokio::fs::write(&config, kept)
        .await
        .expect("its checkouts' keep");
    with_fragua
}
