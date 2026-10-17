//! What `git worktree list --porcelain` says: every checkout linked to a
//! repository, the commit it stands on and the branch it is on.

use std::path::PathBuf;

use yunta_core::CommitSha;

/// One checkout as the repository's list of worktrees names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) path: PathBuf,
    /// The commit it stands on — `None` in a repository with no commit.
    pub(crate) head: Option<CommitSha>,
    /// The full name of the branch it is on — `None` on a detached `HEAD`.
    pub(crate) branch: Option<String>,
    /// Whether git says the checkout is gone from where it was listed.
    pub(crate) prunable: bool,
}

/// Every checkout `listing` names.
pub(crate) fn checkouts(listing: &str) -> Vec<Listed> {
    let mut found = Vec::new();
    let mut current: Option<Listed> = None;
    for line in listing.lines().chain(std::iter::once("")) {
        if let Some(named) = line.strip_prefix("worktree ") {
            current = Some(Listed {
                path: PathBuf::from(named),
                head: None,
                branch: None,
                prunable: false,
            });
        } else if let Some(listed) = current.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                listed.head = head.parse().ok();
            } else if let Some(named) = line.strip_prefix("branch ") {
                listed.branch = Some(named.to_string());
            } else if line.starts_with("prunable") {
                listed.prunable = true;
            } else if line.is_empty() {
                found.extend(current.take());
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: &str = "0123456789012345678901234567890123456789";
    const ONE: &str = "1111111111111111111111111111111111111111";
    const TWO: &str = "2222222222222222222222222222222222222222";

    #[test]
    fn every_checkout_is_read_with_its_commit_and_its_branch() {
        let listing = format!(
            "worktree /repo\nHEAD {MAIN}\nbranch refs/heads/main\n\n\
             worktree /pool/slot-1\nHEAD {ONE}\ndetached\n\n\
             worktree /pool/slot-2\nHEAD {TWO}\nbranch refs/heads/yunta/unit/r/task/T1/2\n\
             prunable gitdir file points to non-existent location\n"
        );

        let read: Vec<(String, Option<String>, Option<String>, bool)> = checkouts(&listing)
            .into_iter()
            .map(|listed| {
                (
                    listed.path.display().to_string(),
                    listed.head.map(|head| head.as_str().to_string()),
                    listed.branch,
                    listed.prunable,
                )
            })
            .collect();

        let at = |sha: &str| Some(sha.to_string());
        assert_eq!(
            read,
            vec![
                (
                    "/repo".into(),
                    at(MAIN),
                    Some("refs/heads/main".into()),
                    false
                ),
                ("/pool/slot-1".into(), at(ONE), None, false),
                (
                    "/pool/slot-2".into(),
                    at(TWO),
                    Some("refs/heads/yunta/unit/r/task/T1/2".into()),
                    true
                ),
            ]
        );
    }
}
