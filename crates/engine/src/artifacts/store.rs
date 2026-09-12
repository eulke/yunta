//! Where a run keeps the bytes of every artifact it holds, and how the
//! `artifacts/` directory is written from them.
//!
//! An artifact is identified by its content, not by where it was
//! written: the bytes live at `objects/<sha256>` and every event that
//! names an artifact names that hash. Two writes of the same content are
//! one object, so a document a node re-submits costs nothing and a
//! mount that copies a sibling's artifact stores no second copy.
//!
//! Reading verifies. An object whose content no longer hashes to its own
//! name is [`ObjectError::Corrupt`], named with both hashes — the run
//! says the bytes are not the bytes it accepted rather than handing a
//! reader something the log never saw.
//!
//! `artifacts/` is the view: a directory the store writes from what it
//! holds and the engine never reads back. A producer's artifacts sit
//! under its node (`artifacts/<node>/<name>`); what the run acquires
//! without a producer — a mount, a promotion, a `document` input — sits
//! at the root (`artifacts/<name>`).

use std::path::{Path, PathBuf};

use yunta_core::{sha256_hex, ContentHash, NodeId, ARTIFACTS_DIR};

/// The run directory that holds every artifact's bytes, by hash.
pub const OBJECTS_DIR: &str = "objects";

/// Why a run cannot hand over the bytes an artifact names.
#[derive(Debug, thiserror::Error)]
pub enum ObjectError {
    #[error(
        "the run holds no object `{hash}` — an artifact names bytes that are not under \
         `{OBJECTS_DIR}/`"
    )]
    Missing { hash: ContentHash },
    /// The one thing a content-addressed store can state and a path
    /// never could: these are not the bytes that were accepted.
    #[error(
        "object `{hash}` holds content that hashes to `{found}` — the bytes under `{OBJECTS_DIR}/` \
         are not the bytes the run accepted"
    )]
    Corrupt {
        hash: ContentHash,
        found: ContentHash,
    },
    #[error("failed to {context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

/// One run's object store, borrowed from its run directory.
pub struct ObjectStore<'a> {
    run_dir: &'a Path,
}

impl<'a> ObjectStore<'a> {
    /// The store of the run rooted at `run_dir`.
    pub fn at(run_dir: &'a Path) -> Self {
        ObjectStore { run_dir }
    }

    /// Stores `bytes` and answers with the hash they are named by.
    ///
    /// Idempotent: content already stored is left exactly as it is, so
    /// storing the same document twice is one object and never an error.
    /// A new object is written through the run's `scratch/` and renamed
    /// into place, which is atomic on one filesystem — nobody ever meets
    /// half an object, and `objects/` never holds a name that is not a
    /// hash.
    pub fn put(&self, bytes: &[u8]) -> std::io::Result<ContentHash> {
        let hash = sha256_hex(bytes);
        let path = self.path_of(&hash);
        if path.exists() {
            return Ok(hash);
        }
        let scratch = self.run_dir.join("scratch");
        std::fs::create_dir_all(&scratch)?;
        std::fs::create_dir_all(path.parent().unwrap_or(self.run_dir))?;
        let mut file = tempfile::NamedTempFile::new_in(&scratch)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.persist(&path).map_err(|e| e.error)?;
        Ok(hash)
    }

    /// The bytes `hash` names, verified against it.
    pub fn get(&self, hash: &ContentHash) -> Result<Vec<u8>, ObjectError> {
        let path = self.path_of(hash);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(ObjectError::Missing { hash: hash.clone() })
            }
            Err(source) => {
                return Err(ObjectError::Io {
                    context: format!("read object `{}`", path.display()),
                    source,
                })
            }
        };
        let found = sha256_hex(&bytes);
        if found != *hash {
            return Err(ObjectError::Corrupt {
                hash: hash.clone(),
                found,
            });
        }
        Ok(bytes)
    }

    /// Writes the `artifacts/` view of the artifact `hash` names, as
    /// `node` produced it under `name`.
    ///
    /// Regenerated from the store every time, so a view somebody
    /// replaced is replaced back; a nested name nests. Writing a view of
    /// bytes the run does not hold is [`ObjectError::Missing`], never a
    /// file with nothing behind it.
    pub fn project(
        &self,
        node: Option<&NodeId>,
        name: &str,
        hash: &ContentHash,
    ) -> Result<(), ObjectError> {
        let bytes = self.get(hash)?;
        let path = self.run_dir.join(view_path(node, name));
        let io = |context: String| move |source| ObjectError::Io { context, source };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(io(format!("create `{}`", parent.display())))?;
        }
        std::fs::write(&path, &bytes).map_err(io(format!("write `{}`", path.display())))
    }

    /// Where the bytes `hash` names live, absolute.
    pub fn path_of(&self, hash: &ContentHash) -> PathBuf {
        self.run_dir.join(OBJECTS_DIR).join(hash.as_str())
    }
}

/// Where the view of one artifact goes, relative to the run directory.
///
/// The one place the layout of `artifacts/` is written down: a
/// producer's artifacts under its node, what the run acquired without a
/// producer at the root.
pub(crate) fn view_path(node: Option<&NodeId>, name: &str) -> PathBuf {
    let mut path = PathBuf::from(ARTIFACTS_DIR);
    if let Some(node) = node {
        path.push(node.as_str());
    }
    path.push(name);
    path
}
