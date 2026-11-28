//! A pack's tree, walked once ([`PackTree`]): every regular file under the pack's own
//! directory, `.git` passed over, and a symbolic link anywhere in it refused, by the entry's
//! own metadata, so a link to a directory is refused as a link to a file is. Whatever reads a
//! pack before a person confirms it reads through the tree walked: the audit, the roles a
//! setup proposes from it, and the vendoring that copies its files. A read stays inside the
//! pack's directory by its path's own parts, and the walk found no link there, so nothing of
//! a pack is read through a link or from outside it.
//!
//! Not the catalog's reading of the packs a repository vendors, which their content hash
//! holds to what was walked as each was installed.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// A pack's directory and every regular file in it, walked once and found to hold no
/// symbolic link. Not the directory alone, which a link inside could lead out of.
#[derive(Debug, Clone)]
pub struct PackTree {
    root: PathBuf,
    /// Each file, by its path under `root`, in order.
    files: BTreeSet<PathBuf>,
}

/// Why a pack's tree could not be walked: a symbolic link in it, by its path under the
/// pack's directory, or an entry the disk would not read.
#[derive(Debug, thiserror::Error)]
pub enum PackWalkError {
    #[error("`{}` is a symbolic link", link.display())]
    Link { link: PathBuf },
    #[error("failed to read `{}`: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl PackTree {
    /// The tree under `root`, a pack's own directory: every regular file in it, `.git`
    /// passed over; refused at the first symbolic link, which is never followed.
    pub fn walk(root: &Path) -> Result<PackTree, PackWalkError> {
        let mut files = BTreeSet::new();
        collect(root, root, &mut files)?;
        Ok(PackTree {
            root: root.to_path_buf(),
            files,
        })
    }

    /// The pack's directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Each regular file of the pack, by its path under its directory, in order.
    pub fn files(&self) -> impl Iterator<Item = &Path> {
        self.files.iter().map(PathBuf::as_path)
    }

    /// The text of the file at `path` under the pack's directory, its parts read as written:
    /// one that leaves the directory, or reaches into its `.git`, is no file of the pack and
    /// is not read. What the disk answers for any other path is its answer, a path the pack
    /// does not hold among it.
    pub fn read(&self, path: &Path) -> std::io::Result<String> {
        let Some(inside) = inside(path) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                NotOfThePack,
            ));
        };
        // blocking: one file of a pack, read as it is audited or proposed from, before a
        // person confirms it; no run waits on it.
        std::fs::read_to_string(self.root.join(inside))
    }
}

/// A path that leaves a pack's directory, or reaches into its `.git`: none of its files.
#[derive(Debug, thiserror::Error)]
#[error("not a file of the pack: it leaves the pack's directory")]
struct NotOfThePack;

/// `path` as a path under a pack's directory, its `.` parts dropped and each `..` taking
/// back the part before it; none where it leaves the directory, starts at a root, or passes
/// through `.git`, which the walk passes over.
fn inside(path: &Path) -> Option<PathBuf> {
    let mut inside = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(name) if name == ".git" => return None,
            Component::Normal(name) => inside.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                if !inside.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(inside)
}

/// Every regular file under `dir`, by its path under `root`, put in `files`; `.git` passed
/// over and a symbolic link refused.
fn collect(root: &Path, dir: &Path, files: &mut BTreeSet<PathBuf>) -> Result<(), PackWalkError> {
    let unread = |path: &Path| {
        let path = path.to_path_buf();
        move |source| PackWalkError::Read { path, source }
    };
    // blocking: a pack's few directories, walked once as it is fetched or hashed; no run
    // waits on it.
    let entries = std::fs::read_dir(dir).map_err(unread(dir))?;
    for entry in entries {
        let path = entry.map_err(unread(dir))?.path();
        if path.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        // blocking: each entry's own metadata, which tells a link without following it.
        let metadata = std::fs::symlink_metadata(&path).map_err(unread(&path))?;
        let under = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        if metadata.file_type().is_symlink() {
            return Err(PackWalkError::Link { link: under });
        }
        if metadata.is_dir() {
            collect(root, &path, files)?;
        } else {
            files.insert(under);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{inside, PackTree, PackWalkError};

    /// A path is read under the pack's directory by its own parts: `.` and a `..` that stays
    /// inside are taken as written; one that climbs out, starts at a root or reaches into
    /// `.git` is no file of the pack.
    #[test]
    fn a_path_is_inside_the_pack_by_its_own_parts() {
        let read = |path: &str| inside(Path::new(path));
        assert_eq!(
            read("workflows/../prompts/./review.md"),
            Some(PathBuf::from("prompts/review.md"))
        );
        assert_eq!(read("workflows/../../secret"), None);
        assert_eq!(read("/etc/passwd"), None);
        assert_eq!(read("workflows/.git/config"), None);
    }

    /// A link anywhere in the tree refuses the walk, by its path under the pack, whatever it
    /// points at; the files are listed by their paths under it, `.git` passed over.
    #[test]
    fn a_link_anywhere_in_the_tree_refuses_the_walk() {
        let pack = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(pack.path().join("workflows")).unwrap();
        std::fs::create_dir_all(pack.path().join(".git")).unwrap();
        std::fs::write(pack.path().join(".git/config"), "").unwrap();
        std::fs::write(pack.path().join("pack.yaml"), "name: a\n").unwrap();
        std::fs::write(pack.path().join("workflows/a.yaml"), "name: a\n").unwrap();

        let tree = PackTree::walk(pack.path()).unwrap();
        let files: Vec<&Path> = tree.files().collect();
        assert_eq!(
            files,
            [Path::new("pack.yaml"), Path::new("workflows/a.yaml")]
        );

        let elsewhere = tempfile::tempdir().unwrap();
        let link = pack.path().join("workflows/linked");
        std::os::unix::fs::symlink(elsewhere.path(), link).unwrap();
        let refused = PackTree::walk(pack.path());
        assert!(
            matches!(&refused, Err(PackWalkError::Link { link }) if link == Path::new("workflows/linked")),
            "{refused:?}"
        );
    }
}
