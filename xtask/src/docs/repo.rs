//! The repository as the docs checker reads it: its tracked files, what a
//! glob selects among them, and what the branch changed. Tracked files, so a
//! build output or a file a person keeps aside is never what a doc points to;
//! and only those on disk, so a file deleted before it is staged is gone.

use std::path::{Component, Path, PathBuf};
use std::process::Command;

use yunta_core::ScopeGlob;

pub(super) struct Repo {
    root: PathBuf,
    /// Every path on disk that git tracks or would track, relative to the
    /// root: an untracked file a change adds counts before it is committed.
    tracked: Vec<String>,
}

impl Repo {
    /// The workspace this crate sits in, with its tracked files listed.
    pub(super) fn open() -> Result<Repo, String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(Path::to_path_buf)
            .ok_or("the xtask crate sits directly under the workspace root")?;
        Repo::at(root)
    }

    pub(super) fn at(root: PathBuf) -> Result<Repo, String> {
        let listed = git(
            &root,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        )?;
        let tracked = listed
            .split('\0')
            .filter(|path| !path.is_empty() && root.join(path).symlink_metadata().is_ok())
            .map(str::to_string)
            .collect();
        Ok(Repo { root, tracked })
    }

    pub(super) fn tracked(&self) -> &[String] {
        &self.tracked
    }

    /// The tracked files `glob` selects, as a scope glob reads it: `*` never
    /// crosses a `/`.
    pub(super) fn selected(&self, glob: &str) -> Result<Vec<&str>, String> {
        let compiled = glob
            .parse::<ScopeGlob>()
            .map_err(|_| format!("`{glob}` is not a glob"))?
            .compiled()
            .compile_matcher();
        Ok(self
            .tracked
            .iter()
            .filter(|path| compiled.is_match(path.as_str()))
            .map(String::as_str)
            .collect())
    }

    /// Whether `path` is a tracked file, or a directory that holds one.
    pub(super) fn holds(&self, path: &str) -> bool {
        let dir = format!("{}/", path.trim_end_matches('/'));
        self.tracked
            .iter()
            .any(|tracked| tracked == path || tracked.starts_with(&dir))
    }

    /// Whether `path` is a tracked file itself, never a directory.
    pub(super) fn tracks(&self, path: &str) -> bool {
        self.tracked.iter().any(|tracked| tracked == path)
    }

    /// `named` as a path from the root. One written from the directory the
    /// command runs in, with `./` or `..` in it, or absolute, is read where it
    /// points; one outside the repository is refused.
    pub(super) fn relative(&self, named: &str) -> Result<String, String> {
        let here = std::env::current_dir()
            .map_err(|error| format!("cannot read the current directory: {error}"))?;
        relative_to(&here, &self.root, named)
    }

    /// The text of the tracked file at `path`.
    pub(super) fn read(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(self.root.join(path))
            .map_err(|error| format!("cannot read `{path}`: {error}"))
    }

    /// The text at `path`, or `None` when the file is not there.
    pub(super) fn read_if_there(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(path)).ok()
    }

    /// The tracked entries under `dir` with their modes, as `ls-files -s`
    /// lists them.
    pub(super) fn modes_under(&self, dir: &str) -> Result<Vec<(String, String)>, String> {
        let listed = git(&self.root, &["ls-files", "-s", "-z", "--", dir])?;
        Ok(listed
            .split('\0')
            .filter_map(|entry| {
                let (fields, path) = entry.split_once('\t')?;
                let mode = fields.split(' ').next()?;
                Some((mode.to_string(), path.to_string()))
            })
            .collect())
    }

    /// What the branch changed: the commits since its merge base with
    /// `origin/main`, what is staged or edited, and what git does not track
    /// yet but does not ignore. A moved file is both its paths.
    pub(super) fn changed(&self) -> Result<Vec<String>, String> {
        let mut paths: Vec<String> = Vec::new();
        if let Ok(base) = git(&self.root, &["merge-base", "HEAD", "origin/main"]) {
            let since = format!("{}..HEAD", base.trim());
            paths.extend(lines(&git(
                &self.root,
                &["diff", "--name-only", "--no-renames", &since],
            )?));
        }
        paths.extend(lines(&git(
            &self.root,
            &["diff", "--name-only", "--no-renames", "HEAD"],
        )?));
        paths.extend(lines(&git(
            &self.root,
            &["ls-files", "--others", "--exclude-standard"],
        )?));
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

/// `named`, read from `here`, as a path from `root`. Each directory is read
/// as the disk has it, so a link in the way resolves as git sees the tree;
/// the file itself is taken as named, since it may be gone.
fn relative_to(here: &Path, root: &Path, named: &str) -> Result<String, String> {
    let mut lexical = PathBuf::new();
    for part in here.join(named).components() {
        match part {
            Component::ParentDir => {
                lexical.pop();
            }
            Component::CurDir => {}
            other => lexical.push(other),
        }
    }
    let full = match (lexical.parent(), lexical.file_name()) {
        (Some(dir), Some(name)) => dir
            .canonicalize()
            .map(|dir| dir.join(name))
            .unwrap_or_else(|_| lexical.clone()),
        _ => lexical.clone(),
    };
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    match full.strip_prefix(&root) {
        Ok(inside) if !inside.as_os_str().is_empty() => {
            Ok(inside.to_string_lossy().replace('\\', "/"))
        }
        _ => Err(format!(
            "`{named}` names no file inside the repository — name the files you change"
        )),
    }
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// git's stdout in `root` on exit 0, or what it said.
fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| format!("git does not start: {error}"))?;
    match output.status.success() {
        true => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        false => Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch repository under the system's temp directory, with one
    /// commit holding `paths`. The name keeps the repositories of two tests
    /// apart.
    fn committed(name: &str, paths: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("yunta-docs-{name}-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).expect("a stale scratch repository is removed");
        }
        for path in paths {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().expect("a file has a directory"))
                .expect("a directory");
            std::fs::write(full, "x\n").expect("a file");
        }
        scratch_git(&root, &["init", "-q"]);
        scratch_git(&root, &["add", "."]);
        scratch_git(
            &root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-q",
                "-m",
                "base",
            ],
        );
        root
    }

    /// git in `root`, reading no configuration but the repository's own.
    fn scratch_git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .status()
            .expect("git starts");
        assert!(status.success(), "git {} fails", args.join(" "));
    }

    #[test]
    fn a_file_deleted_before_it_is_staged_is_gone() {
        let root = committed("deleted", &["src/kept.rs", "src/gone.rs"]);
        std::fs::remove_file(root.join("src/gone.rs")).expect("the file is deleted");
        let repo = Repo::at(root.clone()).expect("the repository opens");
        assert!(repo.holds("src/kept.rs"));
        assert!(!repo.holds("src/gone.rs"));
        std::fs::remove_dir_all(root).expect("the scratch repository is removed");
    }

    #[test]
    fn a_named_file_is_read_from_where_the_command_runs() {
        let root = committed("named", &["src/kept.rs"]);
        let src = root.join("src");
        let from_src = |named: &str| relative_to(&src, &root, named);
        assert_eq!(from_src("kept.rs").as_deref(), Ok("src/kept.rs"));
        assert_eq!(from_src("./kept.rs").as_deref(), Ok("src/kept.rs"));
        assert_eq!(from_src("../src/gone.rs").as_deref(), Ok("src/gone.rs"));
        let absolute = src.join("kept.rs").display().to_string();
        assert_eq!(from_src(&absolute).as_deref(), Ok("src/kept.rs"));
        assert!(from_src("../../elsewhere.rs").is_err());
        assert!(from_src("..").is_err());
        std::fs::remove_dir_all(root).expect("the scratch repository is removed");
    }

    #[test]
    fn a_moved_file_is_both_its_paths_in_what_changed() {
        let root = committed("moved", &["src/old.rs"]);
        scratch_git(&root, &["mv", "src/old.rs", "src/new.rs"]);
        let repo = Repo::at(root.clone()).expect("the repository opens");
        assert_eq!(
            repo.changed().expect("git lists the change"),
            ["src/new.rs", "src/old.rs"]
        );
        std::fs::remove_dir_all(root).expect("the scratch repository is removed");
    }
}
