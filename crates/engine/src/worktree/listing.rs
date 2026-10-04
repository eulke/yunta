//! What `git worktree list --porcelain` says: every checkout linked to a
//! repository, and the branch each one is on.

use std::path::PathBuf;

/// Every checkout `listing` names, with the full name of the branch it is
/// on — `None` for one on a detached `HEAD`.
pub(crate) fn checkouts(listing: &str) -> Vec<(PathBuf, Option<String>)> {
    let mut found = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    for line in listing.lines().chain(std::iter::once("")) {
        if let Some(named) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(named));
        } else if let Some(named) = line.strip_prefix("branch ") {
            branch = Some(named.to_string());
        } else if line.is_empty() {
            if let Some(path) = path.take() {
                found.push((path, branch.take()));
            }
            branch = None;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_checkout_is_read_with_its_branch() {
        let listing = "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\n\
                       worktree /runs/r/unit-worktrees/slot-1\nHEAD def\ndetached\n\n\
                       worktree /runs/r/unit-worktrees/slot-2\nHEAD 123\n\
                       branch refs/heads/yunta/unit/r/task/T1/2\nprunable gitdir file points to non-existent location\n";

        assert_eq!(
            checkouts(listing),
            vec![
                (PathBuf::from("/repo"), Some("refs/heads/main".to_string())),
                (PathBuf::from("/runs/r/unit-worktrees/slot-1"), None),
                (
                    PathBuf::from("/runs/r/unit-worktrees/slot-2"),
                    Some("refs/heads/yunta/unit/r/task/T1/2".to_string())
                ),
            ]
        );
    }
}
