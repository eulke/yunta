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
//! name is [`ObjectProblem::Corrupt`], named with both hashes — the run
//! says the bytes are not the bytes it accepted rather than handing a
//! reader something the log never saw.
//!
//! `artifacts/` is the view: a directory the store writes from what it
//! holds and the engine never reads back. A producer's artifacts sit
//! under its node (`artifacts/<node>/<name>`); what the run acquires
//! without a producer — a mount, a promotion, a `document` input — sits
//! at the root (`artifacts/<name>`).

use std::path::{Path, PathBuf};

use yunta_core::durable::{self, Durability};
use yunta_core::events::ObjectProblem;
use yunta_core::{sha256_hex, ContentHash, NodeId, ARTIFACTS_DIR, OBJECTS_DIR};

/// Why the store cannot hand over the bytes an object names: the object
/// is not what the run accepted, or the store's own I/O failed while it
/// read or wrote the object. The first is a fact about the artifact; the
/// second is an engine step that failed on its own.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("object")]
    Object(ObjectProblem),
    #[error("failed to {context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

impl StoreError {
    /// The sentence a run's own diagnostics read this error as: the object
    /// problem's own sentence, or the store's failed step in its own words.
    pub fn sentence(&self) -> String {
        match self {
            StoreError::Object(problem) => problem.sentence(),
            StoreError::Io { .. } => self.to_string(),
        }
    }
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
    /// The bytes are staged beside their name and linked under it only once
    /// whole, so nobody meets half an object, and the object is synced to
    /// disk before the hash is answered.
    pub async fn put(&self, bytes: &[u8]) -> std::io::Result<ContentHash> {
        self.put_with(bytes, Durability::Synced).await
    }

    /// [`put`](Self::put), with the durability the caller's object needs:
    /// an object a reader verifies against its name, whose loss the reader
    /// would report rather than read past, may skip the sync that `put`
    /// pays for each object.
    pub async fn put_with(
        &self,
        bytes: &[u8],
        durability: Durability,
    ) -> std::io::Result<ContentHash> {
        let hash = sha256_hex(bytes);
        let path = self.path_of(&hash);
        tokio::fs::create_dir_all(path.parent().unwrap_or(self.run_dir)).await?;
        // blocking: one object, written whole under its name. `Ok(false)`:
        // the name is taken, and a name is the hash of the bytes it holds.
        durable::create_new(&path, bytes, durability)?;
        Ok(hash)
    }

    /// Stores `bytes` with every secret `redactor` knows taken out, as
    /// the log takes them out of what it appends — for what a command
    /// printed, which the run keeps and a secret may have reached.
    ///
    /// Stored with [`Durability::Atomic`], so the object is linked whole
    /// without the sync that `put` makes. A command's output is evidence a
    /// reader reads, never the only copy of any work, and the reader
    /// verifies the object against its name: a lost object reads back as
    /// missing or corrupt, never as other bytes.
    pub async fn put_redacted(
        &self,
        bytes: &[u8],
        redactor: &yunta_core::Redactor,
    ) -> std::io::Result<ContentHash> {
        if redactor.is_empty() {
            return self.put_with(bytes, Durability::Atomic).await;
        }
        self.put_with(&redactor.bytes(bytes), Durability::Atomic)
            .await
    }

    /// This store as what a command printed is kept in it: with every
    /// secret `redactor` knows taken out.
    pub fn redacting(self, redactor: &'a yunta_core::Redactor) -> Redacting<'a> {
        Redacting {
            store: self,
            redactor,
        }
    }

    /// The bytes `hash` names, verified against it.
    pub async fn get(&self, hash: &ContentHash) -> Result<Vec<u8>, StoreError> {
        let path = self.path_of(hash);
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(StoreError::Object(ObjectProblem::Missing {
                    hash: hash.clone(),
                }))
            }
            Err(source) => {
                return Err(StoreError::Io {
                    context: format!("read object `{}`", path.display()),
                    source,
                })
            }
        };
        let found = sha256_hex(&bytes);
        if found != *hash {
            return Err(StoreError::Object(ObjectProblem::Corrupt {
                hash: hash.clone(),
                found,
            }));
        }
        Ok(bytes)
    }

    /// Writes the `artifacts/` view of the artifact `hash` names, as
    /// `node` produced it under `name`.
    ///
    /// Regenerated from the store every time, so a view somebody
    /// replaced is replaced back; a nested name nests. Writing a view of
    /// bytes the run does not hold is [`ObjectProblem::Missing`], never a
    /// file with nothing behind it.
    pub async fn project(
        &self,
        node: Option<&NodeId>,
        name: &str,
        hash: &ContentHash,
    ) -> Result<(), StoreError> {
        let bytes = self.get(hash).await?;
        self.write_view(node, name, &bytes).await
    }

    /// Writes `bytes` as `node`'s view under `name`: a projection of an
    /// object, or a view derived from one.
    pub async fn write_view(
        &self,
        node: Option<&NodeId>,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), StoreError> {
        let path = self.run_dir.join(view_path(node, name));
        let io = |context: String| move |source| StoreError::Io { context, source };
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(io(format!("create `{}`", parent.display())))?;
        }
        // blocking: a view is one small file, replaced whole.
        durable::replace(&path, bytes, Durability::Atomic)
            .map_err(io(format!("write `{}`", path.display())))
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
pub fn view_path(node: Option<&NodeId>, name: &str) -> PathBuf {
    let mut path = PathBuf::from(ARTIFACTS_DIR);
    if let Some(node) = node {
        path.push(node.as_str());
    }
    path.push(name);
    path
}

/// A run's object store as what its commands printed is kept in it.
pub struct Redacting<'a> {
    store: ObjectStore<'a>,
    redactor: &'a yunta_core::Redactor,
}

#[async_trait::async_trait]
impl crate::task_cycle::KeepsOutput for Redacting<'_> {
    async fn keep_output(
        &self,
        output: &crate::process::CommandOutput,
    ) -> std::io::Result<ContentHash> {
        self.store.put_redacted(output.bytes(), self.redactor).await
    }

    async fn kept_output(&self, hash: &ContentHash) -> Option<Vec<u8>> {
        self.store.get(hash).await.ok()
    }
}
