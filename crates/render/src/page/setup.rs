//! A repository being set up, as its screen says it before anything is written: the sentence
//! that says so; what the repository is, a row a fact found; who does the work, a row a role
//! the setup proposes with the adapters that would do it, in the order they are written; and
//! the row that writes it all, with what follows it. A fact the setup did not find has no
//! row: `init`'s printed page says what it looked for ([`Initialized`]). The pack the setup
//! installs has its row once fetching it went somewhere ([`OfferedPack`]): to install, already
//! installed, or out of reach.
//!
//! The sentence opens on what the repository did with yunta before ([`RepositoryPast`]), as
//! the field of its runs holds them: a repository whose field holds a run of it has used
//! yunta, and what the sentence can say is that this checkout holds no setup, as a branch
//! without the configuration does. Until the field is read the page says no sentence, so
//! none is said and then taken back.
//!
//! The words a row shares with `init` are owned once: the name and the base are what
//! `init -i` asks them with (`prompt::PROJECT_NAME`, `prompt::BASE_BRANCH`), the language and
//! the forge are what [`Initialized`] says of them, and who git commits as is doctor's
//! `commits as`. The page is one Surco survey ([`Setup::survey`]): the sentence, each list
//! under its heading, and the row that writes at its foot, which drops what follows the write
//! where it cannot hold it. A row being changed opens in place where the pointer stands on it
//! ([`Pointed`]): a role's
//! adapter as a row of words with one chosen where its candidates stood, its model typed in a
//! box under it; the name or the base typed in a box under its row; each box named in its top
//! edge, what an empty line comes to under it. The screen lays the survey out: the name in
//! block letters over it, the horizon under it, the setup's column at a wide screen and what
//! gives way on a narrow one, among it the candidates after each role's first
//! ([`Candidates`]). Its keys are `screen::setup`'s, by the row pointed at.
//!
//! [`Initialized`]: super::Initialized

use surco::components::{Survey, SurveyList, SurveyRow, SurveyValue, TypedInPlace};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::{Fill, Line, Tone};
use surco::look::Look;
use surco::width::cell_width;
use yunta_core::{
    AdapterId, CommandName, Detected, GitHubRepo, ModelName, PackRef, ProbedAdapter, RunnerName,
};

use super::Page;

mod roles;
use crate::prompt::{BASE_BRANCH, PROJECT_NAME};

/// A repository being set up, before anything is written: what it was found to be, who would
/// do its work, and what writing it does. Not `ProjectSetup`, what the answers make.
pub struct Setup<'a> {
    /// What the repository did with yunta before, once the field of its runs is read: what
    /// the sentence opens with. None before, where the page says no sentence.
    pub past: Option<RepositoryPast>,
    /// The name it is written under: as answered, or as detected until it is.
    pub name: &'a str,
    /// The branch its runs start from: as answered, or as detected until it is.
    pub base_branch: &'a str,
    /// What the repository was found to have.
    pub detected: &'a Detected,
    /// Who git commits a run's work as here.
    pub committer: CommitsAs<'a>,
    /// The pack the setup installs, as fetching it went, where the setup offers one.
    pub pack: Option<OfferedPack<'a>>,
    /// Every adapter this build knows, probed: what a role no adapter answers for names.
    pub probed: &'a [ProbedAdapter],
    /// Each role the setup proposes, in the order it proposes them.
    pub roles: &'a [ProposedRole<'a>],
    /// The row the pointer stands on, with what is open on it, where it stands.
    pub pointed: Option<Pointed<'a>>,
}

/// The row of a setup the pointer stands on, with the change open on it where one is: a row
/// is changed in place only where the pointer stands, so the change names its row. Not the
/// answer a change comes to, which the setup's own rows say once it is taken.
#[derive(Debug, Clone)]
pub enum Pointed<'a> {
    /// The name, the line it is changed to typed in a box under it where it is open; what
    /// stands where an empty line is taken is the name the setup holds.
    Name(Option<Typing>),
    /// The base branch, as the name is.
    Base(Option<Typing>),
    /// A role, by its runner, its adapter being chosen where it is open.
    Role(&'a RunnerName, Option<Choosing<'a>>),
    /// The row that writes.
    Write,
}

/// Words being typed in a box: those before the cursor and those after it.
#[derive(Debug, Clone, Default)]
pub struct Typing {
    pub before: String,
    pub after: String,
}

/// A role's adapter being chosen in place among those this machine answers for, in a row of
/// words where its candidates stood, its model typed in a box under it. Made only with one
/// of them chosen ([`Choosing::among`]).
#[derive(Debug, Clone)]
pub struct Choosing<'a> {
    adapters: &'a [&'a AdapterId],
    chosen: usize,
    model: Typing,
    on_the_model: bool,
}

impl<'a> Choosing<'a> {
    /// The adapter at `chosen` among `adapters` being chosen, its `model` typed under it, the
    /// keys typing in the box rather than choosing where `on_the_model`; none where `chosen`
    /// stands past them.
    pub fn among(
        adapters: &'a [&'a AdapterId],
        chosen: usize,
        model: Typing,
        on_the_model: bool,
    ) -> Option<Self> {
        (chosen < adapters.len()).then_some(Choosing {
            adapters,
            chosen,
            model,
            on_the_model,
        })
    }
}

/// What a repository being set up did with yunta before, as the field of its runs holds them:
/// what the setup's sentence opens with. Not whether this checkout holds a setup, which it does
/// not either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryPast {
    /// The field holds no run of it: the repository has never used yunta.
    NeverUsed,
    /// The field holds a run of it: the repository has used yunta, and this checkout holds no
    /// setup.
    Used,
}

impl RepositoryPast {
    /// The clause a setup's sentence opens with.
    pub fn clause(self) -> &'static str {
        match self {
            RepositoryPast::NeverUsed => "This repository has never used yunta.",
            RepositoryPast::Used => "This checkout holds no yunta setup.",
        }
    }
}

/// Who git commits a run's work as in the repository set up, as its row says it: a person by
/// name, or what git said where it names no one. Not doctor's finding of the same, which says
/// the address too.
#[derive(Debug, Clone, Copy)]
pub enum CommitsAs<'a> {
    /// The name git commits as.
    Named(&'a str),
    /// git names no one: what it said, whose last line is its reason.
    Unnamed { git_said: &'a str },
}

/// The pack a setup installs, as fetching it went: in reach, to install as a pack; installed
/// in the repository already; or out of reach, with what fetching it said.
#[derive(Debug, Clone, Copy)]
pub enum OfferedPack<'a> {
    /// Fetched: writing the setup installs it.
    InReach(&'a PackRef),
    /// Vendored in the repository already: writing the setup installs nothing, and the
    /// roles proposed are the installed pack's.
    Installed(&'a PackRef),
    /// Not fetched: why, whose first line its row says.
    OutOfReach { pack: &'a PackRef, why: &'a str },
}

/// A role a setup proposes, and the adapters that would do its work, in the order they are
/// written: the first with the model typed for it, where one was. None where no adapter
/// answers on this machine.
#[derive(Debug, Clone)]
pub struct ProposedRole<'a> {
    pub runner: &'a RunnerName,
    pub candidates: Vec<ProposedCandidate<'a>>,
}

/// An adapter that would do a role's work, with the model it runs where one was typed. Not
/// `RunnerCandidate`, a runner's candidate as a configuration declares it, model and all.
#[derive(Debug, Clone, Copy)]
pub struct ProposedCandidate<'a> {
    pub adapter: &'a AdapterId,
    pub model: Option<&'a ModelName>,
}

/// Which of a role's candidates its row names: the first alone, where the screen gives the
/// others no column, or every one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Candidates {
    First,
    Every,
}

/// The cells a role's first candidate takes on its row before the others: room for an adapter
/// and a model the design writes beside it (`claude-code/claude-sonnet-4-6`), so the others
/// stand in one column down the roles; a box a row is typed in takes as many.
const FIRST_CANDIDATE_CELLS: usize = 32;

impl Setup<'_> {
    /// What the setup says over its rows after the clause it opens with.
    pub const FOUND: &'static str = "Here is what it found. Nothing is written until you say so.";

    /// The heading over what the repository was found to be.
    pub const WHAT_THIS_REPOSITORY_IS: &'static str = "what this repository is";

    /// The heading over the roles the setup proposes.
    pub const WHO_DOES_THE_WORK: &'static str = "who does the work";

    /// The configuration a setup writes, by its path in the repository, as its row that writes
    /// names it.
    pub const CONFIG: &'static str = ".yunta/config.yaml";

    /// What follows the write: the first request.
    pub const THEN_IT_ASKS: &'static str = "then yunta asks what to build";

    /// The row of the language a repository is in, as `init` names it too.
    pub(super) const LANGUAGE: &'static str = "language";

    /// The row of the suite a run measures before its first node.
    const SUITE: &'static str = "suite";

    /// Who git commits a run's work as, as a setup's row is labelled and doctor's check says
    /// it.
    pub(crate) const COMMITS_AS: &'static str = "commits as";

    /// The row of the command a repository lints with, labelled by the name a workflow asks
    /// for it under.
    const LINT: &'static str = "lint";

    /// The row of the forge a run's pull request opens on.
    const FORGE: &'static str = "forge";

    /// The row of the pack a setup installs.
    const WORKFLOW: &'static str = "workflow";

    /// The words before a role's candidates after its first, on a setup's rows and on
    /// what `init` says it declared.
    pub(super) const THEN: &'static str = "then";

    /// What the setup says over its rows in a repository whose past is `past`: the clause
    /// that says it, then what was found.
    pub fn sentence(past: RepositoryPast) -> String {
        format!("{} {}", past.clause(), Self::FOUND)
    }

    /// The page as its screen lays it out: the sentence once the past is known, what this
    /// repository is and who does the work, a row a fact or a role, each role naming the
    /// `candidates` the screen has room for, the row being changed opened in place, and the row
    /// that writes at its foot, at `look`'s width.
    pub fn survey(&self, look: &Look, candidates: Candidates) -> Survey {
        let said = self
            .past
            .map(|past| Line::new().push(Tone::Muted, Self::sentence(past)));
        let facts = self.found().into_iter().map(|fact| self.fact_row(fact));
        let roles = self
            .roles
            .iter()
            .map(|role| self.role_row(role, candidates));
        Survey {
            said,
            lists: vec![
                SurveyList {
                    heading: Self::WHAT_THIS_REPOSITORY_IS.to_string(),
                    rows: facts.collect(),
                },
                SurveyList {
                    heading: Self::WHO_DOES_THE_WORK.to_string(),
                    rows: roles.collect(),
                },
            ],
            foot: self.write_row(look),
        }
    }

    /// What writing the setup does: the configuration written, and the pack installed where it
    /// was fetched.
    pub fn writes(&self) -> String {
        match self.pack {
            Some(OfferedPack::InReach(pack)) => {
                format!("write {} and install {pack}", Self::CONFIG)
            }
            Some(OfferedPack::Installed(_) | OfferedPack::OutOfReach { .. }) | None => {
                format!("write {}", Self::CONFIG)
            }
        }
    }

    /// The row that writes: the pointer where it stands there, the row then filled to its
    /// region's edge; what writing does, and what follows it where the row holds it at
    /// `look`'s width.
    pub fn write_row(&self, look: &Look) -> Line {
        let pointed = matches!(self.pointed, Some(Pointed::Write));
        let said = match pointed {
            true => Tone::Strong,
            false => Tone::Muted,
        };
        let row = Line::new()
            .plain(" ")
            .then(Line::pointed(pointed, look.glyphs))
            .push(said, self.writes());
        let then = format!("  {} {}", look.glyphs.sep(), Self::THEN_IT_ASKS);
        let held = cell_width(&row.text()) + cell_width(&then) <= look.width.cells();
        let row = match held {
            true => row.push(Tone::Muted, then),
            false => row,
        };
        match pointed {
            true => row.filled(Fill::Selection),
            false => row,
        }
    }

    /// The row of `fact`: the pointer where it stands there, and the name or the base typed
    /// in place under it where it is changed, an empty line keeping what the setup holds.
    fn fact_row(&self, fact: Fact) -> SurveyRow {
        let (pointed, typing, kept) = match (&fact.row, &self.pointed) {
            (Some(FactRow::Name), Some(Pointed::Name(typing))) => (true, typing, self.name),
            (Some(FactRow::Base), Some(Pointed::Base(typing))) => (true, typing, self.base_branch),
            _ => (false, &None, ""),
        };
        let typed = typing.as_ref().map(|typing| TypedInPlace {
            label: fact.label.to_string(),
            typed: crate::prompt::typed_row(typing.before.clone(), typing.after.clone()),
            cells: FIRST_CANDIDATE_CELLS,
            hint: Some(crate::prompt::none_keeps(kept)),
            focused: true,
        });
        SurveyRow {
            mark: fact.mark,
            label: fact.label.to_string(),
            value: SurveyValue::Found(fact.said),
            pointed,
            typed,
        }
    }

    /// What the repository was found to be, a fact a row in the order the setup says them:
    /// its name, its language, its suite and its lint, its forge, its base, who git commits as
    /// and the pack the setup installs, each where it was found; each holds unless its row
    /// says otherwise.
    fn found(&self) -> Vec<Fact> {
        let holds = |label: &'static str, said: &str| Fact {
            mark: Mark::Done,
            label,
            said: said.to_string(),
            row: None,
        };
        let changes = |row: FactRow, said: &str| Fact {
            row: Some(row),
            ..holds(row.label(), said)
        };
        let detected = self.detected;
        let lint = detected.commands.get(&CommandName::from_static(Self::LINT));
        [
            Some(changes(FactRow::Name, self.name)),
            detected
                .ecosystem
                .map(|ecosystem| holds(Self::LANGUAGE, ecosystem.language)),
            detected
                .suite
                .as_deref()
                .map(|suite| holds(Self::SUITE, suite)),
            lint.map(|lint| holds(Self::LINT, lint)),
            detected
                .forge
                .as_ref()
                .map(|repo| holds(Self::FORGE, &from_origin(repo))),
            Some(changes(FactRow::Base, self.base_branch)),
            Some(self.committer.fact()),
            self.pack.map(OfferedPack::fact),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// A fact the setup found, as its row says it: its mark, its label and what was found; and
/// which row of those a person changes it is, where it is one.
struct Fact {
    mark: Mark,
    label: &'static str,
    said: String,
    row: Option<FactRow>,
}

/// A fact's row a person changes in place: the name, or the base branch.
#[derive(Debug, Clone, Copy)]
enum FactRow {
    Name,
    Base,
}

impl FactRow {
    /// Its label, as `init -i` asks it.
    fn label(self) -> &'static str {
        match self {
            FactRow::Name => PROJECT_NAME,
            FactRow::Base => BASE_BRANCH,
        }
    }
}

impl Page for Setup<'_> {
    /// The page as one survey, each role naming every candidate.
    fn document(&self, look: &Look) -> Doc {
        Doc::new().with(Block::Survey(self.survey(look, Candidates::Every)))
    }
}

impl CommitsAs<'_> {
    /// Its row: the name git commits as, or why git names no one, a run's first commit
    /// failing until it does.
    fn fact(self) -> Fact {
        let (mark, said) = match self {
            CommitsAs::Named(name) => (Mark::Done, name.to_string()),
            CommitsAs::Unnamed { git_said } => {
                (Mark::Failed, crate::refusal::unnamed_committer(git_said))
            }
        };
        Fact {
            mark,
            label: Setup::COMMITS_AS,
            said,
            row: None,
        }
    }
}

impl OfferedPack<'_> {
    /// Its row: the pack, to install as a pack, installed as one already, or out of reach
    /// with the first line of what fetching it said.
    fn fact(self) -> Fact {
        let (mark, said) = match self {
            OfferedPack::InReach(pack) => (Mark::Done, format!("{pack}, to install as a pack")),
            OfferedPack::Installed(pack) => (Mark::Done, format!("{pack}, installed as a pack")),
            OfferedPack::OutOfReach { pack, why } => (
                Mark::Caution,
                format!(
                    "{pack}, out of reach: {}",
                    why.lines().next().unwrap_or_default()
                ),
            ),
        };
        Fact {
            mark,
            label: Setup::WORKFLOW,
            said,
            row: None,
        }
    }
}

impl ProposedCandidate<'_> {
    /// The candidate as its role's row names it ([`crate::runner::candidate`]): its adapter,
    /// then its model after a slash where one was typed.
    fn said(&self) -> String {
        crate::runner::candidate(self.adapter, self.model)
    }
}

/// The forge `repo` is on, as `origin` names it: a fact a setup and `init` say alike.
pub(super) fn from_origin(repo: &GitHubRepo) -> String {
    format!("github {repo}, from origin")
}
