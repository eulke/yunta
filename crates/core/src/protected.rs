//! What no session of a run writes, and the reason each path is protected.
//!
//! One set answers every question about a protected path: a node's fence
//! refuses it, a request that reaches it is refused whole, and the close of
//! a unit judges its diff against the same set. A set that does not compile
//! is an error when it is built, never a partial set.

use std::path::{Component, Path, PathBuf};

use globset::{GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::glob::{might_overlap, scope_globset, InvalidScopeGlob, ScopeGlob};

/// Why a path is protected. The first reason that holds a path names it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Protection {
    /// The project's own `permissions.paths.deny`.
    ProjectDeny,
    /// The configuration tree of the agent CLI a session runs under.
    AgentConfig,
    /// The instruction files an agent reads: the one reason a person may grant.
    AgentInstructions,
    /// A test a person approved for a spec: no task writes it.
    ApprovedTest,
    /// The run's own git register: `.git`, and what lies under it.
    GitRegister,
}

impl Protection {
    /// Whether a person may grant a path this reason protects. Only the
    /// instruction files: nobody grants the rest.
    pub fn person_may_grant(self) -> bool {
        self == Protection::AgentInstructions
    }
}

/// A loop's close leaves approved tests out: each task judges them when it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    Every,
    LoopClose,
}

/// A changed path a set protects, and the reason it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trespass {
    pub path: PathBuf,
    pub reason: Protection,
}

/// The protected globs, each with its reason, the grants that lift some of
/// them, and the sets compiled from both. Serialized as its entries and
/// grants; reading it compiles the sets again.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(try_from = "Stored", into = "Stored")]
pub struct Protected {
    entries: Vec<Entry>,
    /// The instruction files a person granted, which `lifted` compiles.
    granted: Vec<ScopeGlob>,
    every: GlobSet,
    at_loop_close: GlobSet,
    lifted: GlobSet,
}

/// What a set is stored as: its entries, and the grants that lift it.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Stored {
    entries: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lifted: Vec<ScopeGlob>,
}

/// The protected globs as git pathspecs, for the git calls that stage or
/// clean the ignored paths they cover, split by how far git looks for them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pathspecs {
    /// Globs under a directory they name (`.claude/**`): git looks through
    /// that directory whole, one the project ignores whole included.
    pub rooted: Vec<String>,
    /// Globs that name no directory (`**/AGENTS.md`, `.mcp.json`): git
    /// reaches what it finds without entering a directory the project
    /// ignores whole, so a build output or a dependency tree is never read.
    pub loose: Vec<String>,
}

impl Pathspecs {
    pub fn is_empty(&self) -> bool {
        self.rooted.is_empty() && self.loose.is_empty()
    }
}

/// Serialized as `{reason, glob}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    reason: Protection,
    glob: ScopeGlob,
}

/// Why a protected set does not build.
#[derive(Debug, thiserror::Error)]
pub enum ProtectedError {
    #[error("protected paths do not build into one set: {source}")]
    Build {
        #[source]
        source: globset::Error,
    },
    /// A pattern that does not compile: one the caller passed, or a fixed one.
    #[error(transparent)]
    Pattern(#[from] InvalidScopeGlob),
}

/// What every agent CLI yunta drives reads as its own configuration in a
/// checkout: Claude Code's and Codex's. Protected in every run, whichever
/// adapters the run uses, so no session rewrites what another CLI reads.
pub const AGENT_CONFIG: &[&str] = &[".claude/**", ".mcp.json", ".codex/**", ".agents/**"];

impl Protected {
    /// The one union. The agents' configuration, the instruction files and
    /// the git register are fixed here.
    pub fn new(project: &[ScopeGlob], approved: &[ScopeGlob]) -> Result<Self, ProtectedError> {
        let fixed = |list: &[&str]| -> Result<Vec<ScopeGlob>, ProtectedError> {
            list.iter()
                .map(|glob| glob.parse::<ScopeGlob>().map_err(ProtectedError::from))
                .collect()
        };
        let mut entries = Vec::new();
        for (reason, globs) in [
            (Protection::ProjectDeny, project.to_vec()),
            (Protection::AgentConfig, fixed(AGENT_CONFIG)?),
            (
                Protection::AgentInstructions,
                fixed(&["**/AGENTS.md", "**/CLAUDE.md"])?,
            ),
            (Protection::ApprovedTest, approved.to_vec()),
            (Protection::GitRegister, fixed(&[".git", ".git/**"])?),
        ] {
            entries.extend(globs.into_iter().map(|glob| Entry { reason, glob }));
        }
        Protected::try_from(entries)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every protected glob, in the order the reasons are read.
    pub fn every(&self) -> Vec<ScopeGlob> {
        self.entries
            .iter()
            .map(|entry| entry.glob.clone())
            .collect()
    }

    /// The globs a loop's close judges: every one but the approved tests.
    pub fn at_loop_close(&self) -> Vec<ScopeGlob> {
        self.entries
            .iter()
            .filter(|entry| entry.reason != Protection::ApprovedTest)
            .map(|entry| entry.glob.clone())
            .collect()
    }

    /// The strictest reason that reaches `glob`: one nobody may grant
    /// outranks the instruction files.
    pub fn reached_by(&self, glob: &ScopeGlob) -> Option<Protection> {
        self.entries
            .iter()
            .filter(|entry| might_overlap(glob, &entry.glob))
            .map(|entry| entry.reason)
            .min_by_key(|reason| reason.person_may_grant())
    }

    /// The reason a worktree-relative path is protected, if it is. A lifted
    /// path loses only its instruction reason: another reason still holds it.
    pub fn reason_of(&self, relative: &Path) -> Option<Protection> {
        let lifted = self.lifted.is_match(relative);
        self.every
            .matches(relative)
            .into_iter()
            .filter_map(|index| self.entries.get(index))
            .map(|entry| entry.reason)
            .filter(|reason| !(lifted && reason.person_may_grant()))
            .min()
    }

    /// The set for one node or task, with the instruction files a person
    /// granted to it lifted. The caller passes only the grants the gate
    /// accepted. A grant that does not build lifts nothing: the bound fails closed.
    pub fn lifting(&self, granted: &[ScopeGlob]) -> Protected {
        Protected {
            granted: granted.to_vec(),
            lifted: scope_globset(granted).unwrap_or_else(|_| GlobSet::empty()),
            ..self.clone()
        }
    }

    /// The changed paths the set protects at this closing. A path under a
    /// staged mount is the adapter's own write, never a trespass.
    pub fn trespasses(
        &self,
        changed: &[PathBuf],
        staged: &[PathBuf],
        closing: Closing,
    ) -> Vec<Trespass> {
        let set = match closing {
            Closing::Every => &self.every,
            Closing::LoopClose => &self.at_loop_close,
        };
        changed
            .iter()
            .filter(|path| !staged.iter().any(|mount| path.starts_with(mount)))
            .filter(|path| set.is_match(path))
            .filter_map(|path| {
                Some(Trespass {
                    path: path.clone(),
                    reason: self.reason_of(path)?,
                })
            })
            .collect()
    }

    /// Every glob but the register as a `:(glob)` pathspec, for the git calls
    /// that force-add or clean: rooted when the glob names its directory,
    /// loose when it names none.
    pub fn pathspecs(&self) -> Pathspecs {
        let mut pathspecs = Pathspecs::default();
        for entry in &self.entries {
            if entry.reason == Protection::GitRegister {
                continue;
            }
            let spec = format!(":(glob){}", entry.glob);
            match entry.glob.tree() {
                Some(_) => pathspecs.rooted.push(spec),
                None => pathspecs.loose.push(spec),
            }
        }
        pathspecs
    }
}

/// Both sets are built here; a set that does not build is the error, never a partial set.
impl TryFrom<Vec<Entry>> for Protected {
    type Error = ProtectedError;

    fn try_from(entries: Vec<Entry>) -> Result<Self, ProtectedError> {
        let build = |globs: Vec<&ScopeGlob>| {
            let mut set = GlobSetBuilder::new();
            for glob in globs {
                set.add(glob.compiled().clone());
            }
            set.build()
                .map_err(|source| ProtectedError::Build { source })
        };
        let every = build(entries.iter().map(|entry| &entry.glob).collect())?;
        let at_loop_close = build(
            entries
                .iter()
                .filter(|entry| entry.reason != Protection::ApprovedTest)
                .map(|entry| &entry.glob)
                .collect(),
        )?;
        Ok(Protected {
            entries,
            granted: Vec::new(),
            every,
            at_loop_close,
            lifted: GlobSet::empty(),
        })
    }
}

impl TryFrom<Stored> for Protected {
    type Error = ProtectedError;

    fn try_from(stored: Stored) -> Result<Self, ProtectedError> {
        let set = Protected::try_from(stored.entries)?;
        Ok(match stored.lifted.is_empty() {
            true => set,
            false => set.lifting(&stored.lifted),
        })
    }
}

impl From<Protected> for Stored {
    fn from(protected: Protected) -> Self {
        Stored {
            entries: protected.entries,
            lifted: protected.granted,
        }
    }
}

/// `GlobSet` does not compare, so equality is that of the globs the sets
/// are compiled from.
impl PartialEq for Protected {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries && self.granted == other.granted
    }
}

impl Eq for Protected {}

/// Whether a worktree-relative path is the git register: its first component
/// that is not `.` is `.git`. A linked worktree leaves that name as a file.
pub fn is_register(relative: &Path) -> bool {
    relative
        .components()
        .find(|component| *component != Component::CurDir)
        .is_some_and(|first| first.as_os_str() == ".git")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_register_is_its_first_name_whatever_dots_come_before_it() {
        assert!(is_register(Path::new(".git")));
        assert!(is_register(Path::new("./.git/config")));
        assert!(!is_register(Path::new(".gitignore")));
        assert!(!is_register(Path::new("src/.git")));
    }

    #[test]
    fn the_strictest_reason_that_reaches_a_glob_is_the_one_nobody_may_grant() {
        let set = Protected::new(&[ScopeGlob::from(".github/**")], &[]).unwrap();
        assert_eq!(
            set.reached_by(&ScopeGlob::from("**")),
            Some(Protection::ProjectDeny)
        );
        assert_eq!(
            set.reached_by(&ScopeGlob::from("docs/**")),
            Some(Protection::AgentInstructions)
        );
    }

    #[test]
    fn a_lifted_instruction_file_and_a_staged_path_are_no_trespass() {
        let set = Protected::new(&[], &[ScopeGlob::from("tests/approved/**")]).unwrap();
        let changed = [
            PathBuf::from("AGENTS.md"),
            PathBuf::from("tests/approved/a.rs"),
        ];
        assert_eq!(set.trespasses(&changed, &[], Closing::Every).len(), 2);
        let lifted = set.lifting(&[ScopeGlob::from("AGENTS.md")]);
        assert_eq!(
            lifted.trespasses(&changed, &[], Closing::Every),
            vec![Trespass {
                path: "tests/approved/a.rs".into(),
                reason: Protection::ApprovedTest,
            }]
        );
        let staged = [PathBuf::from("AGENTS.md")];
        assert_eq!(set.trespasses(&changed, &staged, Closing::Every).len(), 1);
    }

    #[test]
    fn a_loop_close_leaves_the_approved_tests_to_their_task() {
        let set = Protected::new(&[], &[ScopeGlob::from("tests/approved/**")]).unwrap();
        let changed = [
            PathBuf::from("AGENTS.md"),
            PathBuf::from("tests/approved/a.rs"),
        ];
        assert_eq!(
            set.trespasses(&changed, &[], Closing::LoopClose),
            vec![Trespass {
                path: "AGENTS.md".into(),
                reason: Protection::AgentInstructions,
            }]
        );
    }

    #[test]
    fn pathspecs_name_every_protection_but_the_register() {
        let set = Protected::new(&[ScopeGlob::from(".github/**")], &[]).unwrap();
        assert_eq!(
            set.pathspecs(),
            Pathspecs {
                rooted: [".github/**", ".claude/**", ".codex/**", ".agents/**"]
                    .map(|glob| format!(":(glob){glob}"))
                    .to_vec(),
                loose: [".mcp.json", "**/AGENTS.md", "**/CLAUDE.md"]
                    .map(|glob| format!(":(glob){glob}"))
                    .to_vec(),
            }
        );
    }

    #[test]
    fn a_lift_removes_the_instruction_reason_and_no_other() {
        let set = Protected::new(&[ScopeGlob::from("**/*.md")], &[]).unwrap();
        let lifted = set.lifting(&[ScopeGlob::from("AGENTS.md")]);
        assert_eq!(
            lifted.reason_of(Path::new("AGENTS.md")),
            Some(Protection::ProjectDeny)
        );
    }

    #[test]
    fn the_set_is_stored_as_its_entries_and_grants_and_built_again_on_read() {
        let set = Protected::new(&[ScopeGlob::from(".github/**")], &[]).unwrap();
        let text = serde_json::to_string(&set).unwrap();
        let read: Protected = serde_json::from_str(&text).unwrap();
        assert_eq!(read, set);
        assert_eq!(
            read.reason_of(Path::new(".github/ci.yml")),
            Some(Protection::ProjectDeny)
        );
        let lifted = set.lifting(&[ScopeGlob::from("AGENTS.md")]);
        assert_ne!(lifted, set);
        let text = serde_json::to_string(&lifted).unwrap();
        let read: Protected = serde_json::from_str(&text).unwrap();
        assert_eq!(read, lifted);
        assert_eq!(read.reason_of(Path::new("AGENTS.md")), None);
    }
}
