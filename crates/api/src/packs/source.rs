//! See [`super`]. Where a pack is fetched from, as a person names it:
//! `<repository>[//<directory>][@<ref>]`. The repository is anything `git
//! clone` takes (a URL, an SSH shorthand, a local path, or a bare `host/path`
//! that `https://` is put before); the directory, after a `//` that is not the
//! `://` of a scheme, is where the pack stands inside it, its root when none is
//! named; the ref, after the last `@` of the path, is what is checked out. The
//! lock records the source without its ref, `<repository>//<directory>`, so an
//! update fetches again from the same directory.
//!
//! The directory is untrusted text held inside the clone: written absolute or
//! with a `..` it is refused before anything is cloned, and once cloned, a
//! symbolic link on the way to it or a directory the repository does not hold
//! is refused too. Then the pack's own tree is walked once, and a symbolic link
//! anywhere in it refuses the pack: its audit, the roles a setup proposes from
//! it and its vendoring all read through that walk, so no file of the pack is
//! read through a link before a person confirms it. Only the pack's own
//! directory is read, audited and vendored; nothing of the clone outside it
//! reaches the project.
//!
//! The repository and the ref are untrusted text too, a lock's among them, which
//! a pull request can edit: one that begins with `-` is refused where a source
//! is read, since git would take it as an option, and the commands that clone
//! and check out end their options before either.
//!
//! What is checked out is a [`Checkout`]: the branch a clone lands on, a ref a
//! person or an update names, which the whole repository is cloned for since it
//! can be a commit, or a tag known to be one, as a setup's is, cloned alone.

use std::path::{Component, Path, PathBuf};

use yunta_core::refusal::{PackRefusal, SourceDirectoryFault, SourcePart};
use yunta_engine::process::Supervision;
use yunta_engine::{PackTree, PackWalkError};

use super::vendor::{self, PackError};
use super::{failed, refused};
use crate::{Error, Failure};

/// Where a pack is fetched from: the repository git clones, and the directory
/// inside it the pack stands at, held inside it.
///
/// Not the ref: what is checked out is asked apart, by an add's `@<ref>` or by
/// an update.
pub(super) struct PackSource {
    /// The source as it was given, without its ref: what a refusal names.
    named: String,
    /// What git clones.
    repository: String,
    /// The pack's directory inside the repository, of plain names only; `None`
    /// for the repository's root.
    directory: Option<PathBuf>,
}

/// What of a pack's repository is checked out.
#[derive(Debug, Clone, Copy)]
pub(super) enum Checkout<'r> {
    /// The branch a clone of the whole repository lands on.
    Landed,
    /// A ref a person or an update names: a tag, a branch or a commit, so the whole
    /// repository is cloned and the ref checked out in it.
    Ref(&'r str),
    /// A tag known to be one, cloned alone, at depth one.
    Tag(&'r str),
}

impl PackSource {
    /// The source a person names to add a pack, `<repository>[//<directory>]
    /// [@<ref>]`, and its ref when one is pinned: refused as [`PackSource::read`]
    /// refuses it, or where the ref begins with `-`.
    pub(super) fn named(spec: &str) -> Result<(PackSource, Option<&str>), Error> {
        let (source, reference) = split_source_and_ref(spec);
        let source = PackSource::read(source)?;
        let reference = reference.map(|reference| source.reference(reference));
        Ok((source, reference.transpose()?))
    }

    /// `reference`, a ref asked of this source: refused where it begins with `-`,
    /// which git would read as an option.
    pub(super) fn reference<'r>(&self, reference: &'r str) -> Result<&'r str, Error> {
        if !reference.starts_with('-') {
            return Ok(reference);
        }
        Err(refused(PackRefusal::ReadsAsAnOption {
            named: self.named.clone(),
            part: SourcePart::Ref(reference.to_string()),
        }))
    }

    /// `source`, `<repository>[//<directory>]` as an add names it without its
    /// ref and as a lock records it: refused where the repository begins with
    /// `-`, which git would read as an option, or the directory is absolute or
    /// climbs with a `..`. A `//` that names nothing is the repository's root.
    pub(super) fn read(source: &str) -> Result<PackSource, Error> {
        let (repository, written) = split_directory(source);
        if repository.starts_with('-') {
            return Err(refused(PackRefusal::ReadsAsAnOption {
                named: source.to_string(),
                part: SourcePart::Repository(repository.to_string()),
            }));
        }
        let refused_for = |why| {
            refused(PackRefusal::SourceDirectory {
                named: source.to_string(),
                directory: written.unwrap_or_default().to_string(),
                why,
            })
        };
        let mut directory = PathBuf::new();
        for part in Path::new(written.unwrap_or_default()).components() {
            match part {
                Component::Normal(name) => directory.push(name),
                Component::CurDir => {}
                Component::ParentDir => return Err(refused_for(SourceDirectoryFault::Climbs)),
                Component::RootDir | Component::Prefix(_) => {
                    return Err(refused_for(SourceDirectoryFault::Absolute))
                }
            }
        }
        Ok(PackSource {
            named: source.to_string(),
            repository: clone_url(repository),
            directory: (!directory.as_os_str().is_empty()).then_some(directory),
        })
    }

    /// What the lock records: the repository, then `//` and the directory
    /// where the pack stands below its root.
    pub(super) fn recorded(&self) -> String {
        match &self.directory {
            Some(directory) => format!("{}//{}", self.repository, directory.display()),
            None => self.repository.clone(),
        }
    }

    /// The repository cloned into a fresh directory, `checkout` checked out,
    /// where the pack stands in it found and its tree walked, refused at a
    /// symbolic link anywhere in it. A local repository written relative is
    /// read from `from`, the repository the pack is installed in.
    pub(super) async fn cloned(
        &self,
        checkout: Checkout<'_>,
        from: &Path,
        supervision: Supervision<'_>,
    ) -> Result<Cloned, Error> {
        let clone =
            tempfile::tempdir().map_err(|source| Failure::new(CloneDirUncreated(source)))?;
        vendor::clone_pack(&self.repository, checkout, from, clone.path(), supervision)
            .await
            .map_err(failed)?;
        let pack = self.pack_in(clone.path())?;
        let tree = PackTree::walk(&pack).map_err(|walked| match walked {
            PackWalkError::Link { link } => refused(PackRefusal::LinkInPack {
                named: self.named.clone(),
                link: link.display().to_string(),
            }),
            PackWalkError::Read { path, source } => failed(PackError::Read { path, source }),
        })?;
        Ok(Cloned { clone, tree })
    }

    /// Where the pack stands in `clone`, a checkout of the repository: refused
    /// where a part of the way to it is a symbolic link, which could lead out
    /// of the clone, or is not a directory the repository holds.
    fn pack_in(&self, clone: &Path) -> Result<PathBuf, Error> {
        let Some(directory) = &self.directory else {
            return Ok(clone.to_path_buf());
        };
        let refused_for = |why| {
            refused(PackRefusal::SourceDirectory {
                named: self.named.clone(),
                directory: directory.display().to_string(),
                why,
            })
        };
        let (mut at, mut walked) = (clone.to_path_buf(), PathBuf::new());
        for name in directory {
            at.push(name);
            walked.push(name);
            let metadata = match std::fs::symlink_metadata(&at) {
                Ok(metadata) => metadata,
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                    return Err(refused_for(SourceDirectoryFault::Missing))
                }
                Err(source) => return Err(failed(PackError::Read { path: at, source })),
            };
            if metadata.file_type().is_symlink() {
                let link = walked.display().to_string();
                return Err(refused_for(SourceDirectoryFault::Linked { link }));
            }
            if !metadata.is_dir() {
                return Err(refused_for(SourceDirectoryFault::Missing));
            }
        }
        Ok(at)
    }
}

/// A repository cloned to fetch a pack from, and the pack's tree in it, walked:
/// the clone is removed when this is dropped.
pub(super) struct Cloned {
    clone: tempfile::TempDir,
    tree: PackTree,
}

impl Cloned {
    /// The clone's root, where git is asked what it checked out.
    pub(super) fn root(&self) -> &Path {
        self.clone.path()
    }

    /// The pack's tree in the clone, walked and found to hold no link: what is
    /// read, audited and vendored.
    pub(super) fn tree(&self) -> &PackTree {
        &self.tree
    }
}

/// `<source>[@<ref>]` (e.g. `github.com/acme/review-pack@v1.2.0`) —
/// split on the last `@` **only when it follows the source's last `/`
/// and what follows it holds no `:`**, which git refuses in a ref, so an
/// SSH shorthand like `git@github.com:acme/repo.git` (whose `@` comes
/// before any `/`) or `git@host:repo.git` (with no `/` at all) is never
/// mistaken for a `source@ref` pin.
fn split_source_and_ref(spec: &str) -> (&str, Option<&str>) {
    let Some(at) = spec.rfind('@') else {
        return (spec, None);
    };
    let (source, reference) = (&spec[..at], &spec[at + 1..]);
    let after_the_path = spec.rfind('/').is_none_or(|slash| at > slash);
    match after_the_path && !reference.contains(':') {
        true => (source, Some(reference)),
        false => (spec, None),
    }
}

/// `<repository>//<directory>`, split on the first `//` after a scheme's
/// `://`, so `https://host/repo` names no directory and
/// `file:///srv/repo//packs/demo` names `packs/demo`.
fn split_directory(source: &str) -> (&str, Option<&str>) {
    let after_scheme = source.find("://").map_or(0, |scheme| scheme + "://".len());
    match source[after_scheme..].find("//") {
        Some(at) => {
            let at = after_scheme + at;
            (&source[..at], Some(&source[at + "//".len()..]))
        }
        None => (source, None),
    }
}

/// A bare `host/path` shorthand (no scheme, no SSH `user@host:` prefix,
/// not already a local filesystem path) gets `https://` prepended —
/// `github.com/acme/review-pack` is exactly this shorthand, never a
/// literally clonable URL on its own. A local path
/// (starts with `.`, `/` or `~`) or anything already carrying a scheme
/// or an `@` (SSH shorthand) passes through untouched.
fn clone_url(source: &str) -> String {
    let looks_local = source.starts_with('.') || source.starts_with('/') || source.starts_with('~');
    let has_scheme = source.contains("://");
    let looks_ssh_shorthand = source.contains('@');
    if looks_local || has_scheme || looks_ssh_shorthand {
        source.to_string()
    } else {
        format!("https://{source}")
    }
}

/// The directory a pack is cloned into could not be made.
#[derive(Debug, thiserror::Error)]
#[error("failed to create a temp directory to clone into: {0}")]
struct CloneDirUncreated(#[source] std::io::Error);

#[cfg(test)]
mod tests;
