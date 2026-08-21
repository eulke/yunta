//! Support for `yunta pack add/remove/list/update` (RFC-0002 §4, T11.2):
//! git-clone-and-vendor mechanics, the `yunta.lock` on-disk convention,
//! and the vendored tree's content hash. The command functions
//! (`commands::pack`) do the printing and argument handling; this
//! module is where the actual filesystem/git work lives, kept separate
//! so it stays testable without going through a CLI process.

use std::path::{Path, PathBuf};

use yunta_core::{sha256_hex, PackLock, PackManifest};

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("git {args}: {detail}")]
    Git { args: String, detail: String },
    #[error("failed to read `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write `{path}`: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("`{path}` has no `pack.yaml` at its root — not a pack")]
    NoManifest { path: PathBuf },
    #[error("`{path}`: {detail}")]
    InvalidManifest { path: PathBuf, detail: String },
    #[error("`{path}` isn't valid UTF-8, can't be hashed as pack content")]
    NonUtf8Path { path: PathBuf },
}

/// `<source>[@<ref>]` (§4's own `github.com/acme/review-pack@v1.2.0`) —
/// split on the last `@` **only when it follows the source's last `/`**,
/// so an SSH shorthand like `git@github.com:acme/repo.git` (whose `@`
/// comes before any `/`) is never mistaken for a `source@ref` pin.
pub fn split_source_and_ref(spec: &str) -> (&str, Option<&str>) {
    let last_slash = spec.rfind('/');
    let at = spec.rfind('@');
    match (at, last_slash) {
        (Some(at), Some(slash)) if at > slash => (&spec[..at], Some(&spec[at + 1..])),
        (Some(at), None) => (&spec[..at], Some(&spec[at + 1..])),
        _ => (spec, None),
    }
}

/// A bare `host/path` shorthand (no scheme, no SSH `user@host:` prefix,
/// not already a local filesystem path) gets `https://` prepended —
/// §4's own example (`github.com/acme/review-pack`) is exactly this
/// shorthand, never a literally clonable URL on its own. A local path
/// (starts with `.`, `/` or `~`) or anything already carrying a scheme
/// or an `@` (SSH shorthand) passes through untouched.
pub fn clone_url(source: &str) -> String {
    let looks_local = source.starts_with('.') || source.starts_with('/') || source.starts_with('~');
    let has_scheme = source.contains("://");
    let looks_ssh_shorthand = source.contains('@');
    if looks_local || has_scheme || looks_ssh_shorthand {
        source.to_string()
    } else {
        format!("https://{source}")
    }
}

async fn run_git(cwd: &Path, args: &[&str]) -> Result<String, PackError> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| PackError::Git {
            args: args.join(" "),
            detail: e.to_string(),
        })?;
    if !output.status.success() {
        return Err(PackError::Git {
            args: args.join(" "),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Clones `url` into `dest` (a fresh, empty directory), checking out
/// `ref_` when given — a full clone, not shallow: `--depth 1` would
/// only work for a ref that's a branch tip, and §4 allows "a tag,
/// branch or commit-ish" indifferently.
pub async fn clone_pack(url: &str, ref_: Option<&str>, dest: &Path) -> Result<(), PackError> {
    run_git(
        Path::new("."),
        &["clone", "--quiet", url, &dest.display().to_string()],
    )
    .await?;
    if let Some(ref_) = ref_ {
        run_git(dest, &["checkout", "--quiet", ref_]).await?;
    }
    Ok(())
}

/// The commit `dest` (an already-cloned working tree) currently has
/// checked out — what `add`/`update` records as the lock entry's
/// `commit`, independent of whether `ref` itself later moves.
pub async fn head_commit(dest: &Path) -> Result<String, PackError> {
    run_git(dest, &["rev-parse", "HEAD"]).await
}

/// The branch `dest` landed on when no explicit ref was requested — the
/// descriptive `ref` a lock entry records for a plain `pack add <url>`
/// with no `@ref` suffix, so `yunta.lock` never has to say "unknown".
pub async fn current_branch(dest: &Path) -> Result<String, PackError> {
    run_git(dest, &["rev-parse", "--abbrev-ref", "HEAD"]).await
}

/// Reads and parses `<dir>/pack.yaml`.
pub fn read_manifest(dir: &Path) -> Result<PackManifest, PackError> {
    let path = dir.join("pack.yaml");
    let contents = std::fs::read_to_string(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            PackError::NoManifest {
                path: dir.to_path_buf(),
            }
        } else {
            PackError::Read {
                path: path.clone(),
                source,
            }
        }
    })?;
    serde_yaml::from_str(&contents).map_err(|e| PackError::InvalidManifest {
        path,
        detail: e.to_string(),
    })
}

/// sha256 over every file under `dir` (skipping `.git`), each
/// contributing `"<relative/path>\0<sha256 of its bytes>\n"` in
/// lexicographic path order — deterministic regardless of the
/// filesystem's own directory-entry order, and independent of file
/// mtimes/permissions surviving a copy (only path + content matter,
/// same as any other content-addressed hash in this codebase).
pub fn hash_tree(dir: &Path) -> Result<String, PackError> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(dir, dir, &mut files)?;
    files.sort();

    let mut digest_input = String::new();
    for relative in &files {
        let full = dir.join(relative);
        let bytes = std::fs::read(&full).map_err(|source| PackError::Read {
            path: full.clone(),
            source,
        })?;
        let relative_str = relative.to_str().ok_or_else(|| PackError::NonUtf8Path {
            path: relative.clone(),
        })?;
        digest_input.push_str(relative_str);
        digest_input.push('\0');
        digest_input.push_str(&sha256_hex(&bytes));
        digest_input.push('\n');
    }
    Ok(sha256_hex(digest_input.as_bytes()))
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), PackError> {
    let entries = std::fs::read_dir(dir).map_err(|source| PackError::Read {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| PackError::Read {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            out.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
    Ok(())
}

/// Copies every file under `src` (skipping `.git`) into `dest`,
/// creating directories as needed — the vendoring step itself,
/// separate from the clone so `add`/`update` can hash the clone (which
/// still has `.git`) and vendor a `.git`-free copy from the same
/// checkout without cloning twice.
pub fn vendor_tree(src: &Path, dest: &Path) -> Result<(), PackError> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(src, src, &mut files)?;
    for relative in files {
        let from = src.join(&relative);
        let to = dest.join(&relative);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|source| PackError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::copy(&from, &to).map_err(|source| PackError::Write { path: to, source })?;
    }
    Ok(())
}

pub fn packs_root(cwd: &Path) -> PathBuf {
    cwd.join(".yunta/packs")
}

pub fn vendor_dir(cwd: &Path, publisher: &str, name: &str) -> PathBuf {
    packs_root(cwd).join(publisher).join(name)
}

pub fn lock_path(cwd: &Path) -> PathBuf {
    cwd.join(".yunta/yunta.lock")
}

pub fn load_lock(cwd: &Path) -> Result<PackLock, PackError> {
    let path = lock_path(cwd);
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_yaml::from_str(&contents).map_err(|e| PackError::InvalidManifest {
            path,
            detail: e.to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PackLock::default()),
        Err(source) => Err(PackError::Read { path, source }),
    }
}

pub fn save_lock(cwd: &Path, lock: &PackLock) -> Result<(), PackError> {
    let path = lock_path(cwd);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| PackError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let yaml = serde_yaml::to_string(lock).expect("PackLock always serializes");
    std::fs::write(&path, yaml).map_err(|source| PackError::Write { path, source })
}
