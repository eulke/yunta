//! The fence breach rule, judged against the coverage a session reported and
//! the confinement it recorded.

use super::*;
use yunta_core::port::PermissionProfile;
use yunta_core::{Commands, Worktree};

/// The record of a read-only session: its commands confined, and no file
/// of the checkout writable through its file tools.
fn read_only_record() -> ConfinementRecord {
    ConfinementRecord {
        profile: PermissionProfile::ReadOnly,
        commands: Commands::Confined,
        worktree: Worktree::Nothing,
        network: false,
    }
}

/// The record of an edit session whose commands are confined and whose file
/// tools write `worktree`.
fn edit_record(worktree: Worktree) -> ConfinementRecord {
    ConfinementRecord {
        profile: PermissionProfile::Edit,
        commands: Commands::Confined,
        worktree,
        network: false,
    }
}

/// The record of a session whose commands are unconfined, as a full profile
/// runs them.
fn unconfined_record() -> ConfinementRecord {
    ConfinementRecord {
        profile: PermissionProfile::Full,
        commands: Commands::Unconfined,
        worktree: Worktree::All,
        network: true,
    }
}

/// A scope check whose only violations are `paths`.
fn violating(paths: &[&str]) -> ScopeCheckResult {
    ScopeCheckResult {
        violations: paths.iter().copied().map(PathBuf::from).collect(),
        ..Default::default()
    }
}

/// The coverage a confined session reports: its commands write the
/// temporary directories, so the coverage is not exact.
fn widened() -> Coverage {
    Coverage::WidenedToRoots {
        roots: vec![PathBuf::from("/tmp")],
    }
}

#[test]
fn an_exact_coverage_with_violations_files_a_breach() {
    let breach = fence_breach(Some(&Coverage::Exact), None, &violating(&["b.txt"]));
    assert_eq!(
        breach.map(|found| found.paths),
        Some(vec![PathBuf::from("b.txt")])
    );
}

#[test]
fn a_widened_coverage_over_a_read_only_record_files_a_breach() {
    let read_only = read_only_record();
    let breach = fence_breach(Some(&widened()), Some(&read_only), &violating(&["b.txt"]));
    assert_eq!(
        breach.map(|found| found.paths),
        Some(vec![PathBuf::from("b.txt")])
    );
}

#[test]
fn a_widened_coverage_over_an_edit_record_files_no_breach() {
    // An edit session may write its whole checkout, or only the files its
    // scope names, and a write it makes is a violation and nothing more.
    for worktree in [Worktree::All, Worktree::ExactFiles] {
        let edit = edit_record(worktree);
        let breach = fence_breach(Some(&widened()), Some(&edit), &violating(&["b.txt"]));
        assert_eq!(breach, None, "a session that may write {worktree:?}");
    }
}

#[test]
fn tools_only_over_an_unconfined_record_files_no_breach() {
    let unconfined = unconfined_record();
    let breach = fence_breach(
        Some(&Coverage::ToolsOnly),
        Some(&unconfined),
        &violating(&["b.txt"]),
    );
    assert_eq!(breach, None);
}

#[test]
fn a_widened_coverage_with_no_record_files_no_breach() {
    let breach = fence_breach(Some(&widened()), None, &violating(&["b.txt"]));
    assert_eq!(breach, None);
}

#[test]
fn no_coverage_files_no_breach() {
    let read_only = read_only_record();
    let breach = fence_breach(None, Some(&read_only), &violating(&["b.txt"]));
    assert_eq!(breach, None);
}

#[test]
fn no_violations_file_no_breach() {
    // The coverage and the record that file a breach for a violation file
    // none for a diff that holds no violation.
    let read_only = read_only_record();
    let breach = fence_breach(
        Some(&widened()),
        Some(&read_only),
        &ScopeCheckResult::default(),
    );
    assert_eq!(breach, None);
}
