//! A repository being set up, as its screen says it: each row its approved frames carry, at
//! the width the screen gives its rows (the setup's column of 92 cells at 120 columns, the
//! main region's 78 at 80 and its 58 at 60), a role naming the candidates after its first
//! from 100 columns; with the pack it installs in reach, and out of it. And the page, whole,
//! in every environment.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use surco::doc::{Block, Doc};
use surco::glyphs::Glyphs;
use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::{Look, Width};
use surco::surface::{Surface, Terminal};
use yunta_core::{
    AdapterId, CommandName, Detected, Ecosystem, GitHubRepo, ModelName, PackRef, ProbedAdapter,
    RunnerName,
};

use crate::page::{
    Candidates, CommitsAs, Initialized, OfferedPack, Page, Pointed, ProposedCandidate,
    ProposedRole, RepositoryPast, Setup, WrittenRunners,
};

mod in_place;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/setup")
}

/// The rows of `frame`, an approved frame of the application's screens, where it stands:
/// among the frames still to draw, or among the screens once the screen that draws it landed.
fn frame(frame: &str) -> Vec<String> {
    let screens = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tui/goldens");
    let file = ["targets", "screens"]
        .iter()
        .map(|dir| screens.join(dir).join(format!("{frame}.txt")))
        .find(|file| file.is_file())
        .unwrap_or_else(|| panic!("no frame `{frame}` among the screens' targets"));
    let text = std::fs::read_to_string(&file).expect("a frame that reads");
    text.lines().map(str::to_string).collect()
}

/// Rows `first` to `last` of `frame`, counted from one, from the cell its rows start at
/// (`margin` cells in), each without the blank cells it ends with.
fn rows_of(frame: &[String], (first, last): (usize, usize), margin: usize) -> Vec<String> {
    frame[first - 1..last]
        .iter()
        .map(|row| {
            let row: String = row.chars().skip(margin).collect();
            row.trim_end().to_string()
        })
        .collect()
}

/// `drawn`'s lines as text, each without the blank cells it ends with.
fn texts(drawn: &[surco::line::Line]) -> Vec<String> {
    drawn
        .iter()
        .map(|line| line.text().trim_end().to_string())
        .collect()
}

/// A look `cells` wide, in `glyphs`, unpainted: as the screen lays the setup's rows out.
fn at(cells: usize, glyphs: Glyphs) -> Look {
    Look {
        glyphs,
        width: Width::exact(cells),
        ..Look::plain()
    }
}

/// A Rust repository on GitHub, as detection finds the design's: its lint, its suite and its
/// formatter, and `origin` on `eulke/yunta`.
fn rust_on_github() -> Detected {
    let command = |name: &'static str, text: &str| (CommandName::from_static(name), text.into());
    Detected {
        ecosystem: Some(Ecosystem {
            language: "rust",
            cache_tip: "share a build cache across worktrees",
        }),
        commands: BTreeMap::from([
            command("lint", "cargo clippy --all-targets -- -D warnings"),
            command("test", "cargo test"),
            command("format", "cargo fmt --check"),
        ]),
        suite: Some("cargo test".to_string()),
        forge: Some("eulke/yunta".parse::<GitHubRepo>().expect("a repository")),
        adapters: Default::default(),
    }
}

/// `id` probed: answering where `healthy` says, not on the PATH otherwise.
fn probed(id: &'static str, healthy: bool) -> ProbedAdapter {
    ProbedAdapter {
        id: AdapterId::from_static(id),
        healthy,
        detail: match healthy {
            true => "2.1.0".to_string(),
            false => "not on PATH".to_string(),
        },
    }
}

/// The adapters this machine answers for, in the order they were found: Claude Code, then
/// Codex.
fn answering() -> Vec<AdapterId> {
    vec![
        AdapterId::from_static("claude-code"),
        AdapterId::from_static("codex"),
    ]
}

/// `runner` proposed on every adapter of `adapters`, in order, with no model.
fn on<'a>(runner: &'a RunnerName, adapters: &'a [AdapterId]) -> ProposedRole<'a> {
    ProposedRole {
        runner,
        candidates: adapters
            .iter()
            .map(|adapter| ProposedCandidate {
                adapter,
                model: None,
            })
            .collect(),
    }
}

/// The roles fragua's workflows name, in the order the setup proposes them.
fn fraguas_roles() -> Vec<RunnerName> {
    [
        "planner",
        "executor",
        "reviewer",
        "reviewer-alt",
        "mechanical",
    ]
    .into_iter()
    .map(RunnerName::from_static)
    .collect()
}

/// What git said fetching a pack from a repository that is not one.
const NOT_A_REPOSITORY: &str = "fatal: '/home/nowhere' does not appear to be a git repository\n\
                                fatal: Could not read from remote repository.\n";

/// The sentence the setup's page opens with, folded as a region `cells` wide folds it.
fn folded(setup: &Setup<'_>, cells: usize) -> Vec<String> {
    let look = at(cells, Glyphs::Unicode);
    let said = setup.survey(&look, Candidates::First).said;
    let said = said.expect("the page opens on its sentence");
    texts(&said.rows_within(cells))
}

/// The setup's rows drawn at `look`, from the heading over what the repository is down to the
/// row that writes, the candidates after a role's first named where the screen is 100
/// columns wide or more.
fn rows(setup: &Setup<'_>, look: Look, candidates: Candidates) -> Vec<String> {
    let survey = Doc::new().with(Block::Survey(setup.survey(&look, candidates)));
    let drawn = texts(&Terminal::on(look).lines(&survey));
    let heading = drawn
        .iter()
        .position(|row| row == Setup::WHAT_THIS_REPOSITORY_IS)
        .expect("the rows open on what the repository is");
    drawn[heading..].to_vec()
}

#[test]
fn a_rust_repository_with_its_forge_a_committer_the_pack_in_reach_and_five_roles_says_each_row_its_frames_carry(
) {
    let (detected, adapters, pack) = (
        rust_on_github(),
        answering(),
        "yunta/fragua".parse::<PackRef>().expect("a pack"),
    );
    let runners = fraguas_roles();
    let roles: Vec<ProposedRole<'_>> = runners.iter().map(|runner| on(runner, &adapters)).collect();
    let probed = [probed("claude-code", true), probed("codex", true)];
    let setup = Setup {
        past: Some(RepositoryPast::NeverUsed),
        name: "yunta",
        base_branch: "main",
        detected: &detected,
        committer: CommitsAs::Named("Marta Ruiz"),
        pack: Some(OfferedPack::InReach(&pack)),
        probed: &probed,
        roles: &roles,
        pointed: Some(Pointed::Write),
    };
    let wide = frame("setup.unicode-120");
    assert_eq!(
        rows_of(&wide, (6, 6), 0)[0].trim(),
        Setup::sentence(RepositoryPast::NeverUsed)
    );
    assert_eq!(
        rows(&setup, at(92, Glyphs::Unicode), Candidates::Every),
        rows_of(&wide, (8, 25), 14)
    );
    assert_eq!(
        rows(&setup, at(92, Glyphs::Ascii), Candidates::Every),
        rows_of(&frame("setup.ascii-120"), (8, 25), 14)
    );
    let narrow = frame("setup.unicode-80");
    assert_eq!(folded(&setup, 78), rows_of(&narrow, (3, 4), 1));
    assert_eq!(
        rows(&setup, at(78, Glyphs::Unicode), Candidates::First),
        rows_of(&narrow, (5, 22), 1)
    );
    let least = rows(&setup, at(58, Glyphs::Unicode), Candidates::First);
    let window = rows_of(&frame("setup.unicode-60"), (4, 11), 1);
    assert_eq!(least[least.len() - window.len()..], window[..]);
}

/// The sentence opens on what the field holds of the repository's runs: the design's where
/// it holds none, that this checkout holds no setup where it holds one. Before the field is
/// read the page says no sentence, and opens on its rows.
#[test]
fn the_sentence_opens_on_the_repositorys_past_and_waits_for_it() {
    assert_eq!(
        Setup::sentence(RepositoryPast::NeverUsed),
        "This repository has never used yunta. Here is what it found. Nothing is written until \
         you say so."
    );
    assert_eq!(
        Setup::sentence(RepositoryPast::Used),
        "This checkout holds no yunta setup. Here is what it found. Nothing is written until \
         you say so."
    );
    let detected = Detected::default();
    let unread = Setup {
        past: None,
        name: "web",
        base_branch: "main",
        detected: &detected,
        committer: CommitsAs::Named("Marta Ruiz"),
        pack: None,
        probed: &[],
        roles: &[],
        pointed: None,
    };
    let look = at(92, Glyphs::Unicode);
    let drawn = Terminal::on(look).draw(&unread.document(&look));
    assert!(
        unread.survey(&look, Candidates::Every).said.is_none(),
        "{drawn}"
    );
    assert!(drawn.starts_with(Setup::WHAT_THIS_REPOSITORY_IS), "{drawn}");
}

/// The keys a legend row of `frame` names before the far ones, in `glyphs`' words.
fn own_keys(frame: &[String]) -> String {
    let legend = frame.last().expect("a frame ends on its legend").trim();
    let own = legend.split("   ").next().unwrap_or_default();
    own.trim_end_matches(" · ? keys").to_string()
}

#[test]
fn a_setup_and_what_it_wrote_name_the_keys_their_frames_name() {
    for (environment, glyphs) in [
        ("unicode-120", Glyphs::Unicode),
        ("ascii-120", Glyphs::Ascii),
    ] {
        for setup in ["setup", "setup-unreachable"] {
            let frame = frame(&format!("{setup}.{environment}"));
            assert_eq!(
                super::screen::spelled_in(&crate::screen::setup_legend(), glyphs),
                own_keys(&frame)
            );
        }
        let changing = frame(&format!("setup-changing.{environment}"));
        assert_eq!(
            super::screen::spelled_in(
                &crate::screen::choosing_legend(crate::screen::Kept::Detected),
                glyphs
            ),
            own_keys(&changing)
        );
        let written = frame(&format!("setup-written.{environment}"));
        assert_eq!(
            super::screen::spelled_in(&crate::screen::written_legend(), glyphs),
            own_keys(&written)
        );
        let audit = frame(&format!("setup-audit.{environment}"));
        assert_eq!(
            super::screen::spelled_in(&crate::screen::setup_audit_legend(), glyphs),
            own_keys(&audit)
        );
    }
    let narrow = frame("setup.unicode-80");
    assert_eq!(
        super::screen::spelled_in(&crate::screen::setup_legend(), Glyphs::Unicode),
        own_keys(&narrow)
    );
}

#[test]
fn with_the_pack_out_of_reach_the_workflow_row_says_why_and_the_write_row_installs_nothing() {
    let (detected, adapters, pack) = (
        rust_on_github(),
        answering(),
        "yunta/fragua".parse::<PackRef>().expect("a pack"),
    );
    let implementer = RunnerName::PROPOSED;
    let roles = [on(&implementer, &adapters)];
    let probed = [probed("claude-code", true), probed("codex", true)];
    let setup = Setup {
        past: Some(RepositoryPast::NeverUsed),
        name: "yunta",
        base_branch: "main",
        detected: &detected,
        committer: CommitsAs::Named("Marta Ruiz"),
        pack: Some(OfferedPack::OutOfReach {
            pack: &pack,
            why: NOT_A_REPOSITORY,
        }),
        probed: &probed,
        roles: &roles,
        pointed: Some(Pointed::Write),
    };
    assert_eq!(
        rows(&setup, at(92, Glyphs::Unicode), Candidates::Every),
        rows_of(&frame("setup-unreachable.unicode-120"), (8, 22), 14)
    );
    assert_eq!(
        rows(&setup, at(92, Glyphs::Ascii), Candidates::Every),
        rows_of(&frame("setup-unreachable.ascii-120"), (8, 22), 14)
    );
    let narrow = frame("setup-unreachable.unicode-80");
    assert_eq!(folded(&setup, 78), rows_of(&narrow, (6, 7), 1));
    assert_eq!(
        rows(&setup, at(78, Glyphs::Unicode), Candidates::First),
        rows_of(&narrow, (8, 22), 1)
    );
    let least = rows(&setup, at(58, Glyphs::Unicode), Candidates::First);
    let window = rows_of(&frame("setup-unreachable.unicode-60"), (4, 12), 1);
    assert_eq!(least[least.len() - window.len()..], window[..]);
}

/// `page` drawn whole in every environment, held to its goldens under `name`.
fn holds_its_goldens(name: &str, page: &Setup<'_>) {
    for environment in &ENVIRONMENTS {
        let look = Look::of(environment);
        let drawn = Terminal::on(look).draw(&page.document(&look));
        assert_golden(&environment.golden(&goldens(), name), &drawn);
    }
}

#[test]
fn the_page_with_the_pack_in_reach_and_a_role_pointed_matches_its_goldens() {
    let (rust, adapters, pack) = (
        rust_on_github(),
        answering(),
        "yunta/fragua".parse::<PackRef>().expect("a pack"),
    );
    let opus = "opus".parse::<ModelName>().expect("a model");
    let runners = fraguas_roles();
    let mut roles: Vec<ProposedRole<'_>> =
        runners.iter().map(|runner| on(runner, &adapters)).collect();
    roles[0].candidates[0].model = Some(&opus);
    let answered = [probed("claude-code", true), probed("codex", true)];
    let page = Setup {
        past: Some(RepositoryPast::NeverUsed),
        name: "yunta",
        base_branch: "main",
        detected: &rust,
        committer: CommitsAs::Named("Marta Ruiz"),
        pack: Some(OfferedPack::InReach(&pack)),
        probed: &answered,
        roles: &roles,
        pointed: Some(Pointed::Role(&runners[0], None)),
    };
    holds_its_goldens("in-reach", &page);
}

#[test]
fn the_page_with_the_pack_out_of_reach_and_no_one_git_commits_as_matches_its_goldens() {
    let (rust, adapters, pack) = (
        rust_on_github(),
        answering(),
        "yunta/fragua".parse::<PackRef>().expect("a pack"),
    );
    let (planner, opus) = (
        RunnerName::from_static("planner"),
        "opus".parse::<ModelName>().expect("a model"),
    );
    let mut roles = [on(&planner, &adapters)];
    roles[0].candidates[0].model = Some(&opus);
    let answered = [probed("claude-code", true), probed("codex", true)];
    let page = Setup {
        past: Some(RepositoryPast::NeverUsed),
        name: "yunta",
        base_branch: "main",
        detected: &rust,
        committer: CommitsAs::Unnamed {
            git_said: "fatal: unable to auto-detect email address",
        },
        pack: Some(OfferedPack::OutOfReach {
            pack: &pack,
            why: NOT_A_REPOSITORY,
        }),
        probed: &answered,
        roles: &roles,
        pointed: Some(Pointed::Write),
    };
    holds_its_goldens("out-of-reach", &page);
}

#[test]
fn the_page_where_no_adapter_answers_matches_its_goldens() {
    let (bare, implementer) = (Detected::default(), RunnerName::PROPOSED);
    let unanswered = [ProposedRole {
        runner: &implementer,
        candidates: Vec::new(),
    }];
    let unanswering = [probed("claude-code", false), probed("codex", false)];
    let page = Setup {
        past: Some(RepositoryPast::NeverUsed),
        name: "project",
        base_branch: "main",
        detected: &bare,
        committer: CommitsAs::Named("Marta Ruiz"),
        pack: None,
        probed: &unanswering,
        roles: &unanswered,
        pointed: None,
    };
    holds_its_goldens("unanswered", &page);
}

#[test]
fn a_setup_and_what_init_wrote_say_the_language_never_the_ecosystems_name() {
    let detected = Detected {
        ecosystem: Some(Ecosystem {
            language: "typescript",
            cache_tip: "share a package cache across worktrees",
        }),
        ..Detected::default()
    };
    let look = at(92, Glyphs::Unicode);
    let setup = Setup {
        past: Some(RepositoryPast::NeverUsed),
        name: "web",
        base_branch: "main",
        detected: &detected,
        committer: CommitsAs::Named("Marta Ruiz"),
        pack: None,
        probed: &[],
        roles: &[],
        pointed: None,
    };
    let rows = rows(&setup, look, Candidates::Every).join("\n");
    assert!(rows.contains("\n  ✓ language    typescript\n"), "{rows}");
    let (wrote, nothing) = (vec![".yunta/config.yaml".to_string()], Vec::new());
    let implementer = RunnerName::PROPOSED;
    let on_claude = [yunta_core::RunnerCandidate {
        adapter: AdapterId::from_static("claude-code"),
        model: None,
        agent: None,
        effort: None,
    }];
    let initialized = Initialized {
        repo: Path::new("/home/me/web"),
        wrote: &wrote,
        skipped: &nothing,
        detected: &detected,
        probed: &[],
        runners: WrittenRunners::Declared(vec![(&implementer, &on_claude[..])]),
        default: Some(&implementer),
    };
    let page = Terminal::on(look).draw(&initialized.document(&look));
    assert!(
        page.contains("  language     typescript: wrote 0 project commands"),
        "{page}"
    );
    assert!(!page.contains("node"), "{page}");
}
