//! What a session may write and reach, resolved once.
//!
//! A session's sandbox, the judgement its file tools make and the coverage
//! the engine reports all answer one question: what the session may write.
//! [`Confinement::of`] answers it from the profile, the adapter's edit
//! sandbox, whether the edits are held to a declared scope, and the network,
//! and every other reader takes that value rather than deciding again. Temp
//! is not a field: `$TMPDIR` and `/tmp` are writable in every session.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::fence::{Coverage, Fenced};
use crate::port::PermissionProfile;
use crate::protected::Protected;
use crate::{ScopeGlob, Unbuildable};

/// The adapter's `sandbox` setting for `edit` sessions.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum EditSandbox {
    #[default]
    WorkspaceWrite,
    ReadOnly,
    DangerFullAccess,
}

/// What the commands may do; the worktree is what tools may write (judged by the fence's globs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Commands {
    Confined,
    Unconfined,
}

impl Commands {
    /// What a session's commands may do under its profile and its adapter's
    /// `edit` sandbox. Only `full`, and an `edit` session whose sandbox is
    /// `danger-full-access`, run unconfined. `Confinement::of` and the
    /// engine's `check` both read this, so they cannot disagree.
    pub fn of(profile: PermissionProfile, edit: EditSandbox) -> Self {
        match (profile, edit) {
            (PermissionProfile::Full, _)
            | (PermissionProfile::Edit, EditSandbox::DangerFullAccess) => Commands::Unconfined,
            (PermissionProfile::ReadOnly, _) | (PermissionProfile::Edit, _) => Commands::Confined,
        }
    }
}

/// What the session's file tools may write under the checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Worktree {
    /// Nothing under the checkout.
    Nothing,
    /// The files the node's scope names.
    ExactFiles,
    /// Every file under the checkout.
    All,
}

impl Worktree {
    /// What a session's file tools may write under the checkout, under its
    /// profile and its adapter's `edit` sandbox. A read-only profile, and an
    /// `edit` session under a read-only sandbox, write nothing; `full` writes
    /// all. An `edit` session writes the exact files its scope names when its
    /// edits are held to one, and all of them otherwise. `Confinement::of` and
    /// the engine's `check` both read this, so they cannot disagree.
    pub fn of(profile: PermissionProfile, edit: EditSandbox, scoped: bool) -> Self {
        let declared = if scoped {
            Worktree::ExactFiles
        } else {
            Worktree::All
        };
        match (profile, edit) {
            (PermissionProfile::ReadOnly, _) | (PermissionProfile::Edit, EditSandbox::ReadOnly) => {
                Worktree::Nothing
            }
            (PermissionProfile::Full, _) => Worktree::All,
            (PermissionProfile::Edit, _) => declared,
        }
    }
}

/// What one session may reach. Resolved once per session; every adapter
/// maps it exactly or refuses it with [`Unbuildable`].
#[derive(Debug, Clone)]
pub struct Confinement {
    pub profile: PermissionProfile,
    pub commands: Commands,
    pub worktree: Worktree,
    /// The artifact directory, and the shared ones unless read-only.
    pub roots: Vec<PathBuf>,
    /// Whether the commands may reach the network. A session that writes
    /// no worktree never has it, whatever it asks for.
    pub network: bool,
    /// Written by no session of the run; the sandbox denies its literal trees.
    pub protected: Arc<Protected>,
    /// The git directory the run's repository keeps its register in: no
    /// session writes it, wherever it lies, so a commit is the engine's.
    pub git_register: Option<PathBuf>,
}

/// `$TMPDIR` and `/tmp`, writable in every session.
pub fn temp_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![std::env::temp_dir(), PathBuf::from("/tmp")];
    dirs.dedup();
    dirs
}

impl PermissionProfile {
    /// Whether a session of this profile may write under the checkout at all.
    pub fn writes_worktree(self) -> bool {
        self != PermissionProfile::ReadOnly
    }
}

/// What `Confinement::of` reads for one session; the caller gathers it.
pub struct ConfinementInput<'a> {
    pub profile: PermissionProfile,
    pub edit: EditSandbox,
    /// Whether the session's edits are held to a declared scope.
    pub scoped: bool,
    pub network: bool,
    pub checkout: &'a Path,
    pub artifact_dir: Option<&'a Path>,
    pub shared_dirs: &'a [PathBuf],
    pub protected: Arc<Protected>,
    /// The run's git register, when the run knows its repository.
    pub git_register: Option<&'a Path>,
}

impl Confinement {
    /// The one resolution. Refuses an unconfined session whose network is closed,
    /// and a shared directory inside the checkout.
    pub fn of(input: ConfinementInput<'_>) -> Result<Self, Unbuildable> {
        let ConfinementInput {
            profile,
            edit,
            scoped,
            network,
            checkout,
            artifact_dir,
            shared_dirs,
            protected,
            git_register,
        } = input;
        let commands = Commands::of(profile, edit);
        let worktree = Worktree::of(profile, edit, scoped);
        if commands == Commands::Unconfined && !network {
            return Err(Unbuildable::NetworkUnclosable);
        }
        if let Some(dir) = shared_dirs.iter().find(|dir| dir.starts_with(checkout)) {
            return Err(Unbuildable::SharedDirInCheckout { dir: dir.clone() });
        }
        let mut roots: Vec<PathBuf> = artifact_dir.map(Path::to_path_buf).into_iter().collect();
        if profile != PermissionProfile::ReadOnly {
            roots.extend(shared_dirs.iter().cloned());
        }
        let network = network && worktree != Worktree::Nothing;
        Ok(Confinement {
            profile,
            commands,
            worktree,
            roots,
            network,
            protected,
            git_register: git_register.map(Path::to_path_buf),
        })
    }

    /// Whether the session's file tools may write under the checkout at all.
    pub fn writes_worktree(&self) -> bool {
        self.worktree != Worktree::Nothing
    }

    /// Coverage of commands, derived here and never declared. A confined
    /// command's coverage always names temp; an unconfined one has none.
    pub fn coverage(&self, cwd: &Path) -> Coverage {
        let worktree = self.writes_worktree().then(|| cwd.to_path_buf());
        let commands = (self.commands == Commands::Confined).then(|| {
            Fenced::Roots(
                worktree
                    .into_iter()
                    .chain(self.roots.iter().cloned())
                    .chain(temp_dirs())
                    .collect(),
            )
        });
        Coverage::of(Fenced::Exact, commands)
    }

    /// Whether a command's write reaches `target`, judged by directory as a
    /// sandbox judges it: a denied tree wins over every grant, the temporary
    /// directories' included.
    pub fn command_may_write(&self, cwd: &Path, target: &Path) -> bool {
        if self.commands == Commands::Unconfined {
            return true;
        }
        if self
            .protected_trees(cwd)
            .iter()
            .any(|tree| target.starts_with(tree))
        {
            return false;
        }
        (self.writes_worktree() && target.starts_with(cwd))
            || self
                .roots
                .iter()
                .chain(&temp_dirs())
                .any(|dir| target.starts_with(dir))
    }

    /// The trees the sandbox denies: under `cwd`, the directory of every
    /// protected glob that selects every path under one; and the run's git
    /// register wherever it lies, which a temporary directory's grant would
    /// cover otherwise. A protected file, a pattern that selects part of a
    /// directory and a pattern that names no directory are judged by the
    /// rail and the close instead, since denying their directory would deny
    /// paths nothing protects.
    pub fn protected_trees(&self, cwd: &Path) -> Vec<PathBuf> {
        let mut trees: Vec<PathBuf> = self
            .protected
            .every()
            .iter()
            .filter_map(ScopeGlob::whole_tree)
            .map(|tree| cwd.join(tree))
            .collect();
        if let Some(register) = self
            .git_register
            .as_ref()
            .filter(|register| !trees.contains(register))
        {
            trees.push(register.clone());
        }
        trees
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ConfinementRecord;

    fn checkout() -> PathBuf {
        PathBuf::from("/work/run-1/task-1")
    }

    /// The confinement of a session over the checkout, with no artifact directory.
    fn confine(
        profile: PermissionProfile,
        edit: EditSandbox,
        network: bool,
        shared_dirs: &[PathBuf],
    ) -> Result<Confinement, Unbuildable> {
        Confinement::of(ConfinementInput {
            profile,
            edit,
            scoped: false,
            network,
            checkout: &checkout(),
            artifact_dir: None,
            shared_dirs,
            protected: Arc::new(Protected::default()),
            git_register: None,
        })
    }

    #[test]
    fn an_unconfined_edit_session_that_closes_its_network_is_refused() {
        let refused = confine(
            PermissionProfile::Edit,
            EditSandbox::DangerFullAccess,
            false,
            &[],
        );
        assert!(matches!(refused, Err(Unbuildable::NetworkUnclosable)));
    }

    #[test]
    fn a_read_only_session_names_temp_in_its_coverage() {
        let confinement = confine(
            PermissionProfile::ReadOnly,
            EditSandbox::default(),
            true,
            &[],
        )
        .expect("a read-only session is buildable");
        let Coverage::WidenedToRoots { roots } = confinement.coverage(&checkout()) else {
            panic!("a read-only session widens its coverage to the roots it names");
        };
        assert!(
            temp_dirs().iter().all(|dir| roots.contains(dir)),
            "{roots:?}"
        );
    }

    #[test]
    fn shared_dirs_enter_only_the_sessions_that_write() {
        let cache = PathBuf::from("/work/cache");
        let read_only = confine(
            PermissionProfile::ReadOnly,
            EditSandbox::default(),
            false,
            std::slice::from_ref(&cache),
        )
        .expect("a read-only session is buildable");
        assert!(!read_only.roots.contains(&cache));

        let writing = confine(
            PermissionProfile::Edit,
            EditSandbox::default(),
            false,
            std::slice::from_ref(&cache),
        )
        .expect("a writing session is buildable");
        assert!(writing.roots.contains(&cache));
    }

    #[test]
    fn a_shared_dir_inside_the_checkout_is_refused() {
        let inside = checkout().join("cache");
        let refused = confine(
            PermissionProfile::Edit,
            EditSandbox::default(),
            false,
            std::slice::from_ref(&inside),
        );
        assert!(matches!(
            refused,
            Err(Unbuildable::SharedDirInCheckout { dir }) if dir == inside
        ));
    }

    #[test]
    fn a_session_that_writes_no_worktree_is_given_no_network_whatever_it_asks() {
        let read_only = confine(
            PermissionProfile::ReadOnly,
            EditSandbox::default(),
            true,
            &[],
        )
        .expect("a read-only session is buildable");
        assert!(!read_only.network);

        let writing = confine(PermissionProfile::Edit, EditSandbox::default(), true, &[])
            .expect("a writing session is buildable");
        assert!(writing.network);
    }

    #[test]
    fn a_protected_file_or_part_of_a_directory_denies_no_directory_to_the_sandbox() {
        let protected = Protected::new(
            &[
                ScopeGlob::from("config/secrets.yaml"),
                ScopeGlob::from("src/a*.rs"),
            ],
            &[],
        )
        .expect("the project's denies build");
        let cwd = checkout();
        let confinement = Confinement::of(ConfinementInput {
            profile: PermissionProfile::Edit,
            edit: EditSandbox::WorkspaceWrite,
            scoped: false,
            network: false,
            checkout: &cwd,
            artifact_dir: None,
            shared_dirs: &[],
            protected: Arc::new(protected),
            git_register: None,
        })
        .expect("a writing session is buildable");
        let trees = confinement.protected_trees(&cwd);
        assert!(!trees.contains(&cwd.join("config")));
        assert!(!trees.contains(&cwd.join("src")));
        assert!(trees.contains(&cwd.join(".claude")));
        assert!(trees.contains(&cwd.join(".git")));
        assert!(confinement.command_may_write(&cwd, &cwd.join("config/other.yaml")));
        assert!(!confinement.command_may_write(&cwd, &cwd.join(".claude/settings.json")));
    }

    #[test]
    fn a_checkout_is_fenced_only_under_confined_commands_that_write_no_worktree() {
        let fenced = |profile, edit| {
            let confinement = confine(profile, edit, true, &[]).expect("a buildable session");
            ConfinementRecord::of(&confinement).fences_the_checkout()
        };
        assert!(fenced(
            PermissionProfile::ReadOnly,
            EditSandbox::WorkspaceWrite
        ));
        assert!(fenced(PermissionProfile::Edit, EditSandbox::ReadOnly));
        assert!(!fenced(
            PermissionProfile::Edit,
            EditSandbox::WorkspaceWrite
        ));
        assert!(!fenced(
            PermissionProfile::Edit,
            EditSandbox::DangerFullAccess
        ));
        assert!(!fenced(PermissionProfile::Full, EditSandbox::ReadOnly));
    }

    #[test]
    fn a_worktree_is_written_as_its_profile_sandbox_and_scope_allow() {
        assert_eq!(
            Worktree::of(
                PermissionProfile::ReadOnly,
                EditSandbox::WorkspaceWrite,
                true
            ),
            Worktree::Nothing
        );
        assert_eq!(
            Worktree::of(PermissionProfile::Edit, EditSandbox::ReadOnly, true),
            Worktree::Nothing
        );
        assert_eq!(
            Worktree::of(PermissionProfile::Edit, EditSandbox::WorkspaceWrite, true),
            Worktree::ExactFiles
        );
        assert_eq!(
            Worktree::of(PermissionProfile::Edit, EditSandbox::WorkspaceWrite, false),
            Worktree::All
        );
        assert_eq!(
            Worktree::of(PermissionProfile::Full, EditSandbox::ReadOnly, false),
            Worktree::All
        );
    }

    /// A writing session over a checkout under the temporary directory, with
    /// the agent config tree protected and the run's register beside it.
    fn under_temp() -> (PathBuf, PathBuf, Confinement) {
        let temp = std::env::temp_dir();
        let cwd = temp.join("pool/slot-1");
        let register = temp.join("repo/.git");
        let protected = Protected::new(&[], &[]).expect("the fixed set builds");
        let confinement = Confinement::of(ConfinementInput {
            profile: PermissionProfile::Edit,
            edit: EditSandbox::WorkspaceWrite,
            scoped: false,
            network: false,
            checkout: &cwd,
            artifact_dir: None,
            shared_dirs: &[],
            protected: Arc::new(protected),
            git_register: Some(&register),
        })
        .expect("a writing session is buildable");
        (cwd, register, confinement)
    }

    #[test]
    fn the_runs_git_register_is_written_by_no_command_even_under_temp() {
        let (cwd, register, confinement) = under_temp();
        assert!(confinement.protected_trees(&cwd).contains(&register));
        assert!(!confinement.command_may_write(&cwd, &register.join("refs/heads/main")));
        assert!(confinement.command_may_write(&cwd, &std::env::temp_dir().join("scratch.txt")));
    }

    #[test]
    fn a_protected_tree_of_a_checkout_under_temp_is_written_by_no_command() {
        let (cwd, _, confinement) = under_temp();
        assert!(!confinement.command_may_write(&cwd, &cwd.join(".claude/settings.json")));
        assert!(confinement.command_may_write(&cwd, &cwd.join("src/a.rs")));
    }
}
