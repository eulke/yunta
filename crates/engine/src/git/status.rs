//! Where a checkout stands, read from one `git status`: its commit, its
//! branch, and whether anything in it is uncommitted.

use std::path::Path;

use yunta_core::CommitSha;

use super::{read, GitError};
use crate::process::Supervision;

/// Where a checkout stands, and whether anything in it is uncommitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The commit `HEAD` names; `None` when the status lists none.
    pub head: Option<CommitSha>,
    /// The branch `HEAD` names; `None` on a detached `HEAD`.
    pub branch: Option<String>,
    /// Whether the status lists no entry beyond its headers: nothing
    /// uncommitted outside the paths the call excluded.
    pub clean: bool,
}

/// Where the checkout at `dir` stands and whether it is clean, from one
/// `status --porcelain=v2 --branch`. Each name in `excluding` is a path the
/// answer leaves out, such as the request file a session writes for the
/// engine to read.
pub async fn tree_state(
    dir: &Path,
    excluding: &[&str],
    sup: Supervision<'_>,
) -> Result<Status, GitError> {
    let excluded: Vec<String> = excluding
        .iter()
        .map(|name| format!(":(exclude){name}"))
        .collect();
    let mut args = vec!["status", "--porcelain=v2", "--branch", "--", "."];
    args.extend(excluded.iter().map(String::as_str));
    Ok(read_status(&read(dir, &args, sup).await?))
}

/// What `git status --porcelain=v2 --branch` printed, read.
fn read_status(printed: &str) -> Status {
    let mut head = None;
    let mut branch = None;
    let mut clean = true;
    for line in printed.lines() {
        if let Some(oid) = line.strip_prefix("# branch.oid ") {
            head = oid.parse().ok();
        } else if let Some(named) = line.strip_prefix("# branch.head ") {
            branch = (named != "(detached)").then(|| named.to_string());
        } else if !line.starts_with('#') && !line.is_empty() {
            clean = false;
        }
    }
    Status {
        head,
        branch,
        clean,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: &str = "1111111111111111111111111111111111111111";

    #[test]
    fn a_status_reads_where_a_checkout_stands_and_whether_it_is_clean() {
        let detached = format!("# branch.oid {AT}\n# branch.head (detached)\n");
        let dirty = format!("# branch.oid {AT}\n# branch.head yunta/run/r\n? new.txt\n");

        let detached = read_status(&detached);
        let dirty = read_status(&dirty);

        assert_eq!(detached.head.unwrap().as_str(), AT);
        assert_eq!(detached.branch, None);
        assert!(detached.clean);
        assert_eq!(dirty.branch.as_deref(), Some("yunta/run/r"));
        assert!(!dirty.clean);
    }
}
