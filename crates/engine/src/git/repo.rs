//! The repository a checkout belongs to, as git names it: the common git
//! directory its worktrees share, its remotes and branches, who commits in
//! it, and the files a commit holds that name a word.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use yunta_core::RunId;

use super::{change, exit_error, lossy, read, run, split_once, Effect, GitError};
use crate::process::Supervision;

/// The git directory every worktree of the repository at `repo` shares,
/// absolute and canonical: one answer for the main checkout and for each
/// linked worktree. A `.git` that can be read answers without git; otherwise
/// git is asked.
pub async fn common_dir(repo: &Path, sup: Supervision<'_>) -> Result<PathBuf, GitError> {
    let dir = match dot_git_common_dir(repo).await {
        Some(found) => found,
        None => {
            let said = read(repo, &["rev-parse", "--git-common-dir"], sup).await?;
            repo.join(said.trim())
        }
    };
    Ok(tokio::fs::canonicalize(&dir).await.unwrap_or(dir))
}

/// The files `commit` holds that name each of `words` as a whole word, by
/// word, among the files `globs` select from the top of the repository:
/// one `git grep` however many words. Every file is searched as text,
/// whatever its attributes say of it, and one with a NUL in it is named
/// too: what reads the files tells code from bytes. Paths are the
/// repository's, wherever in it `dir` stands. A word no file names is left
/// out; git answers a search that finds nothing with exit 1 and nothing on
/// stderr.
///
/// What git prints is read the same whatever the person's settings say:
/// no color, no line or column numbers, paths from the top, and no file of
/// a submodule.
pub async fn words_in(
    dir: &Path,
    commit: &str,
    words: &[&str],
    globs: &[String],
    sup: Supervision<'_>,
) -> Result<BTreeMap<String, BTreeSet<String>>, GitError> {
    // With no word git would read the commit as its pattern, and with no
    // glob it would search every file: neither asks a question.
    if words.is_empty() || globs.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut args = vec![
        "grep",
        "--no-color",
        "--no-column",
        "--no-line-number",
        "--full-name",
        "--no-recurse-submodules",
        "-a",
        "-o",
        "-z",
        "-w",
        "-F",
    ];
    for word in words {
        args.extend(["-e", *word]);
    }
    let selected: Vec<String> = globs.iter().map(|glob| format!(":(top){glob}")).collect();
    args.extend([commit, "--"]);
    args.extend(selected.iter().map(String::as_str));
    let output = run(dir, &args, sup).await?;
    if output.status.code() == Some(1) && output.stderr.is_empty() {
        return Ok(BTreeMap::new());
    }
    if !output.status.success() {
        return Err(exit_error(dir, &args, &output));
    }
    Ok(by_word(&output.stdout, commit))
}

/// What a `grep -a -o -z` printed, by word. Each match is one record, a
/// binary file's too: the path git names under `<commit>:`, up to a NUL,
/// then the word it matched, up to the line break that ends the record. A
/// path is read to its NUL, so one that holds a line break is read whole.
fn by_word(printed: &[u8], commit: &str) -> BTreeMap<String, BTreeSet<String>> {
    let prefix = format!("{commit}:");
    let mut named: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut rest = printed;
    while let Some((name, after)) = split_once(rest, 0) {
        let (word, after) = split_once(after, b'\n').unwrap_or((after, &[]));
        let name = lossy(name);
        let path = name.strip_prefix(&prefix).unwrap_or(&name);
        named
            .entry(lossy(word))
            .or_default()
            .insert(path.to_string());
        rest = after;
    }
    named
}

/// Who a commit made in `repo` is by — `Name <email>`, as git resolves its
/// author and committer from config and environment. Both identities are
/// read, so a repository that cannot name its author is an error too.
pub async fn committer(repo: &Path, sup: Supervision<'_>) -> Result<String, GitError> {
    read(repo, &["var", "GIT_AUTHOR_IDENT"], sup).await?;
    let ident = read(repo, &["var", "GIT_COMMITTER_IDENT"], sup).await?;
    // `Name <email> <epoch> <zone>`: who, without when.
    Ok(ident
        .trim()
        .rsplitn(3, ' ')
        .last()
        .unwrap_or_default()
        .to_string())
}

/// Pushes what `repo` has checked out to `branch` on `remote`, and sets it as
/// the checkout's upstream. Never forced: a remote branch that moved
/// elsewhere refuses the push, and a push of what the remote already holds
/// changes nothing.
pub async fn push_branch(
    repo: &Path,
    remote: &str,
    branch: &str,
    sup: Supervision<'_>,
) -> Result<(), GitError> {
    let refspec = format!("HEAD:refs/heads/{branch}");
    change(
        repo,
        Effect::Refs,
        &["push", "-q", "-u", remote, &refspec],
        sup,
    )
    .await?;
    Ok(())
}

/// The URL `remote` names in `repo`, or `None` when git lists no remote of
/// that name. A failure to ask git is an error, not an absent remote.
pub async fn remote_url(
    repo: &Path,
    remote: &str,
    sup: Supervision<'_>,
) -> Result<Option<String>, GitError> {
    let listed = read(repo, &["remote"], sup).await?;
    if !listed.lines().any(|name| name.trim() == remote) {
        return Ok(None);
    }
    let url = read(repo, &["remote", "get-url", remote], sup).await?;
    Ok(Some(url.trim().to_string()))
}

/// Every run that has its own branch in the repository at `repo`, read in
/// one listing: what places a run made before runs recorded the repository
/// they were created in.
pub async fn run_branches(repo: &Path, sup: Supervision<'_>) -> Result<BTreeSet<RunId>, GitError> {
    let prefix = format!("{}/", crate::worktree::RUN_BRANCHES);
    let said = read(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            &format!("refs/heads/{prefix}"),
        ],
        sup,
    )
    .await?;
    Ok(said
        .lines()
        .filter_map(|branch| branch.trim().strip_prefix(&prefix))
        .filter_map(|id| id.parse().ok())
        .collect())
}

/// The common git directory, read off the `.git` of the checkout at `repo` as
/// git reads it: a directory is the common directory itself, and a file names
/// an admin directory whose `commondir` names the common one. `None` when that
/// does not answer.
async fn dot_git_common_dir(repo: &Path) -> Option<PathBuf> {
    let dot_git = repo.join(".git");
    if tokio::fs::metadata(&dot_git).await.ok()?.is_dir() {
        return Some(dot_git);
    }
    let pointer = tokio::fs::read_to_string(&dot_git).await.ok()?;
    let admin = repo.join(pointer.strip_prefix("gitdir:")?.trim());
    let common = tokio::fs::read_to_string(admin.join("commondir"))
        .await
        .ok()?;
    Some(admin.join(common.trim()))
}
