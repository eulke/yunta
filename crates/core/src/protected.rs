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

mod links;

pub use links::{resolved, LoadedLink};

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

    /// Whether a link at a path this reason protects carries the reason to
    /// what it resolves to: what a CLI loads, it loads through a link too.
    fn follows_links(self) -> bool {
        matches!(
            self,
            Protection::AgentConfig | Protection::AgentInstructions
        )
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
    /// The paths held on the way, each as itself: git is asked about one
    /// only when a link or a file stands there below real directories, and
    /// never looks under a directory there.
    pub points: Vec<PathBuf>,
}

impl Pathspecs {
    pub fn is_empty(&self) -> bool {
        self.rooted.is_empty() && self.loose.is_empty() && self.points.is_empty()
    }
}

/// Serialized as `{reason, glob}`, with `on_the_way` when it is set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    reason: Protection,
    glob: ScopeGlob,
    /// A path a CLI's lookup passes on the way to what it loads: the root of
    /// a tree it loads, a directory above one, or a path a link's resolution
    /// walks through. It is held as itself, since a link or a file put in its
    /// place leads the lookup elsewhere; what lies under it is not protected
    /// for its sake. A session is not told of it, and the git calls that take
    /// in ignored paths do not look under it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    on_the_way: bool,
}

impl Entry {
    /// An entry that protects every path its glob selects.
    fn of(reason: Protection, glob: ScopeGlob) -> Self {
        Entry {
            reason,
            glob,
            on_the_way: false,
        }
    }

    /// An entry that holds one path a lookup passes as itself.
    fn on_the_way(reason: Protection, path: &Path) -> Result<Self, ProtectedError> {
        Ok(Entry {
            reason,
            glob: ScopeGlob::exact(path)?,
            on_the_way: true,
        })
    }
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

/// What every agent CLI yunta drives loads from a checkout as its own
/// configuration: Claude Code's tree and MCP file, Codex's tree, and the
/// skills root Codex scans. Protected in every run, whichever adapters the
/// run uses, so no session rewrites what another CLI loads. The rest of
/// `.agents/` is the project's: no CLI loads it.
pub const AGENT_CONFIG: &[&str] = &[".claude/**", ".mcp.json", ".codex/**", ".agents/skills/**"];

impl Protected {
    /// The one union. The agents' configuration, the instruction files and
    /// the git register are fixed here, and so is the root of each tree a
    /// CLI loads, with every directory above it, on the way.
    pub fn new(project: &[ScopeGlob], approved: &[ScopeGlob]) -> Result<Self, ProtectedError> {
        let fixed = |list: &[&str]| -> Result<Vec<ScopeGlob>, ProtectedError> {
            list.iter()
                .map(|glob| glob.parse::<ScopeGlob>().map_err(ProtectedError::from))
                .collect()
        };
        let config = fixed(AGENT_CONFIG)?;
        let mut entries = Vec::new();
        for (reason, globs) in [
            (Protection::ProjectDeny, project.to_vec()),
            (Protection::AgentConfig, config.clone()),
            (
                Protection::AgentInstructions,
                fixed(&["**/AGENTS.md", "**/CLAUDE.md"])?,
            ),
            (Protection::ApprovedTest, approved.to_vec()),
            (Protection::GitRegister, fixed(&[".git", ".git/**"])?),
        ] {
            entries.extend(globs.into_iter().map(|glob| Entry::of(reason, glob)));
        }
        for root in links::roots_of(&config) {
            entries.push(Entry::on_the_way(Protection::AgentConfig, &root)?);
        }
        Protected::try_from(entries)
    }

    /// The set a run reads: [`Protected::new`]'s, grown by what the links its
    /// base commit holds where an agent CLI loads carry.
    pub fn of_run(
        project: &[ScopeGlob],
        approved: &[ScopeGlob],
        links: &[LoadedLink],
    ) -> Result<Self, ProtectedError> {
        Protected::new(project, approved)?.following(links)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every protected glob, in the order the reasons are read: what a
    /// session is told it may not write, and what a sandbox denies. A path
    /// held only on the way is left out.
    pub fn every(&self) -> Vec<ScopeGlob> {
        self.entries
            .iter()
            .filter(|entry| !entry.on_the_way)
            .map(|entry| entry.glob.clone())
            .collect()
    }

    /// The globs a loop's close judges: every one but the approved tests.
    pub fn at_loop_close(&self) -> Vec<ScopeGlob> {
        self.entries
            .iter()
            .filter(|entry| !entry.on_the_way && entry.reason != Protection::ApprovedTest)
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

    /// What a person granted the node or task this set is for that lifts one
    /// of its protections: the instruction files among [`Protected::every`]
    /// it may write all the same. A grant that reaches no instruction file
    /// only widens the scope, so it is not here; nor is any grant when the
    /// grants did not build, since they then lift nothing.
    pub fn granted(&self) -> Vec<ScopeGlob> {
        if self.lifted.is_empty() {
            return Vec::new();
        }
        self.granted
            .iter()
            .filter(|glob| {
                self.entries.iter().any(|entry| {
                    entry.reason.person_may_grant() && might_overlap(glob, &entry.glob)
                })
            })
            .cloned()
            .collect()
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
    /// loose when it names none. A path held on the way is a point instead,
    /// since a pathspec that names a directory takes in what lies under it,
    /// which that path does not protect.
    pub fn pathspecs(&self) -> Pathspecs {
        let mut pathspecs = Pathspecs::default();
        for entry in &self.entries {
            if entry.on_the_way {
                let point = entry.glob.exact_path();
                if let Some(point) = point.filter(|point| !pathspecs.points.contains(point)) {
                    pathspecs.points.push(point);
                }
                continue;
            }
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
mod tests;
