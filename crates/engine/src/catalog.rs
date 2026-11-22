//! What a workflow name means. A bare name always means the repository's own
//! `.yunta/workflows/`. A `publisher/name` reference means the repository's
//! own file at that path when there is one, so a local workflow shadows a
//! pack's, and otherwise a workflow the publisher's installed packs declare.
//! Within a publisher, `name` is the basename of a file a pack declares in
//! `contents.workflows`, not the pack's own `name`, which is packaging and
//! versioning metadata (`review-pack`, `1.2.0`) distinct from the files it
//! ships (`review.yaml`).
//!
//! A project reads packs from two stores ([`yunta_core::PackStores`]): its
//! own, then the user's. A user's pack the project also installs, by
//! `publisher/name`, is shadowed whole: the project reads its own copy and
//! nothing of the user's. The first store whose packs declare a name answers
//! it, so the user's store never changes what the project's packs already
//! answer, and two packs of the store that answers are ambiguous rather than
//! settled by install order. The user's store is read only when the
//! project's leaves a name unanswered, so a broken pack there breaks nothing
//! the project answers. Agents and skills are named the same way, and the
//! `org` knowledge layer merges the stores' files by the same order.
//!
//! [`packs`] holds both rules once: one function lists, per store, the pack
//! directories a project reads, shadowing applied, and one asks the stores in
//! order for the first answer. The alternative it rejects is a loop per kind
//! of name, each deciding again which store comes first and what shadows
//! what. A new kind of name a pack ships reads through `pack_declaring`; a
//! new store is one more in `PackStores::in_order`.

mod packs;

use std::path::{Path, PathBuf};

use yunta_core::{Env, PackManifest, PackName, PackRef, PackStore, PackStores, Publisher};

pub(crate) use packs::{declared_knowledge, pack_declaring};
pub use packs::{installed_publishers, packs_for_publisher, PublisherPacks, StoredPack};

/// The file a pack's manifest is, in the pack's own directory.
const PACK_MANIFEST: &str = "pack.yaml";

#[derive(Debug, Clone, thiserror::Error)]
pub enum CatalogError {
    #[error(
        "no workflow `{name}`: not in the repo catalog ({repo_path}), and no pack under \
         publisher `{publisher}` declares a workflow named `{workflow}`"
    )]
    NotFound {
        name: String,
        repo_path: PathBuf,
        publisher: String,
        workflow: String,
    },
    #[error("no workflow `{name}`: not in the repo catalog ({repo_path})")]
    NotFoundNoPacksDir { name: String, repo_path: PathBuf },
    #[error(
        "`{name}` is ambiguous: {count} packs under publisher `{publisher}` each declare a \
         workflow named `{workflow}` ({candidates}); disambiguate by installing only one, or by \
         giving the repo its own `{workflow}.yaml` that names the one you mean"
    )]
    Ambiguous {
        name: String,
        publisher: String,
        workflow: String,
        count: usize,
        candidates: String,
    },
    #[error(
        "`{name}` is not a workflow reference; a reference is a bare name or `publisher/name`, \
         each one path segment: no further `/`, no `\\`, not `.` or `..`, not empty"
    )]
    InvalidName { name: String },
    #[error("pack manifest `{path}` cannot be read: {detail}")]
    Unreadable { path: PathBuf, detail: String },
    #[error("pack manifest `{path}` is malformed: {detail}")]
    Malformed { path: PathBuf, detail: String },
    /// A workflow a pack declares lies outside the pack's directory, so it
    /// is not resolved at all.
    #[error("pack `{pack}` declares `{path}`, which leaves the pack's own directory")]
    Escapes { pack: String, path: String },
}

/// Where a resolved workflow file actually came from: the repository, or a
/// pack in one store. `check`'s cross-pack composition rule needs this to
/// tell "still inside the same pack" from "left it", which a bare
/// [`PathBuf`] can't say on its own; two copies of one pack in two stores
/// are two origins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowOrigin {
    Repo,
    Pack {
        publisher: Publisher,
        pack_name: PackName,
        store: PackStore,
    },
}

impl WorkflowOrigin {
    /// The directory of the pack a pack origin names, in its store; `None`
    /// for the repository.
    pub fn pack_dir(&self) -> Option<PathBuf> {
        match self {
            WorkflowOrigin::Repo => None,
            WorkflowOrigin::Pack {
                publisher,
                pack_name,
                store,
            } => Some(store.pack_dir(&PackRef::new(publisher.clone(), pack_name.clone()))),
        }
    }

    /// The manifest of the pack a pack origin names, which judges the pack
    /// wherever it is installed; `None` for the repository.
    pub fn manifest_file(&self) -> Option<PathBuf> {
        self.pack_dir().map(|dir| dir.join(PACK_MANIFEST))
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedWorkflow {
    pub path: PathBuf,
    pub origin: WorkflowOrigin,
}

/// A path a pack's manifest declares, with the directory of the pack it is
/// relative to and the pack's label. A freeze checks that the path stays
/// inside the pack before it joins it; a checkout reader joins it as it
/// stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclaredPath {
    /// The pack's label, `publisher/name`.
    pub(crate) pack: String,
    pub(crate) pack_dir: PathBuf,
    /// The path as the manifest writes it, relative to `pack_dir`.
    pub(crate) declared: String,
}

impl DeclaredPath {
    pub(crate) fn of_pack(pack_dir: PathBuf, manifest: &PackManifest, declared: String) -> Self {
        Self {
            pack: format!("{}/{}", manifest.publisher, manifest.name),
            pack_dir,
            declared,
        }
    }

    /// The declared path joined onto the pack's directory, with no check.
    pub(crate) fn joined(&self) -> PathBuf {
        self.pack_dir.join(&self.declared)
    }

    /// The declared path joined onto the pack's directory when it stays
    /// inside the pack; `None` when it leaves it. The one containment rule
    /// every reader of a pack's paths asks.
    pub(crate) fn contained(&self) -> Option<PathBuf> {
        yunta_core::stays_inside(&self.declared).then(|| self.joined())
    }
}

/// Where a workflow name resolves, before a pack's declared path is joined
/// onto the pack's directory.
pub(crate) enum Named {
    /// The repo's own file.
    Repo(PathBuf),
    /// A file a pack declares, with the origin that says which pack.
    Pack {
        declared: DeclaredPath,
        origin: WorkflowOrigin,
    },
}

/// Resolves a `use:`/`yunta run` target name against the repo catalog
/// first, packs second (packs are the bottom layer, never shadowing
/// something the repo already names). `ambient` is the environment the
/// user's store is found in; `None` reads only the project's packs. A
/// pack's declared path that leaves the pack is refused, naming the pack and
/// the path, so no reader loads a file outside the pack that declares it.
pub fn resolve_workflow(
    repo_root: &Path,
    ambient: Option<&Env>,
    name: &str,
) -> Result<ResolvedWorkflow, CatalogError> {
    Ok(match name_workflow(repo_root, ambient, name)? {
        Named::Repo(path) => ResolvedWorkflow {
            path,
            origin: WorkflowOrigin::Repo,
        },
        Named::Pack { declared, origin } => ResolvedWorkflow {
            path: declared.contained().ok_or_else(|| CatalogError::Escapes {
                pack: declared.pack.clone(),
                path: declared.declared.clone(),
            })?,
            origin,
        },
    })
}

/// The lookup [`resolve_workflow`] makes, with a pack's declared path left
/// unjoined: the freeze checks that it stays inside its pack first.
pub(crate) fn name_workflow(
    repo_root: &Path,
    ambient: Option<&Env>,
    name: &str,
) -> Result<Named, CatalogError> {
    if !is_reference(name) {
        return Err(CatalogError::InvalidName {
            name: name.to_string(),
        });
    }
    let repo_path = repo_root
        .join(".yunta/workflows")
        .join(format!("{name}.yaml"));
    if repo_path.is_file() {
        return Ok(Named::Repo(repo_path));
    }

    let Some((publisher, workflow)) = name.split_once('/') else {
        return Err(CatalogError::NotFoundNoPacksDir {
            name: name.to_string(),
            repo_path,
        });
    };
    let Ok(publisher_id) = publisher.parse::<Publisher>() else {
        return Err(CatalogError::InvalidName {
            name: name.to_string(),
        });
    };

    let stores = PackStores::of(repo_root, ambient);
    let candidates = packs::declaring_packs(&stores, &publisher_id, workflow)?;
    let (pack, declared) = choose(name, repo_path, publisher, workflow, candidates)?;
    Ok(Named::Pack {
        declared,
        origin: pack.origin(),
    })
}

/// Whether `name` is a bare name or `publisher/name`, each one path segment.
/// Both forms are joined onto paths: a segment that is not one would walk
/// out of the catalog instead of naming something in it.
fn is_reference(name: &str) -> bool {
    match name.split_once('/') {
        Some((publisher, workflow)) => {
            publisher.parse::<Publisher>().is_ok() && yunta_core::is_path_segment(workflow)
        }
        None => yunta_core::is_path_segment(name),
    }
}

/// The one pack a publisher's name resolves to: two packs declaring it are
/// ambiguous, and no pack declaring it is `NotFound`.
fn choose(
    name: &str,
    repo_path: PathBuf,
    publisher: &str,
    workflow: &str,
    mut candidates: Vec<(StoredPack, DeclaredPath)>,
) -> Result<(StoredPack, DeclaredPath), CatalogError> {
    if candidates.len() > 1 {
        return Err(CatalogError::Ambiguous {
            name: name.to_string(),
            publisher: publisher.to_string(),
            workflow: workflow.to_string(),
            count: candidates.len(),
            candidates: candidates
                .iter()
                .map(|(pack, _)| pack.manifest.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    // Exactly one match resolves; an empty set is `NotFound` — the
    // `ok_or_else` here is the zero case, so taking the single element
    // needs no unwrap.
    candidates.pop().ok_or_else(|| CatalogError::NotFound {
        name: name.to_string(),
        repo_path,
        publisher: publisher.to_string(),
        workflow: workflow.to_string(),
    })
}

/// Where `workflow_path` actually lives: the origin `check_workflow_refs`'s
/// caller must supply for its *top-level* workflow, since that function only
/// ever sees a parsed [`Workflow`](yunta_core::Workflow) value with no memory
/// of the file it came from. A path under the packs of either store the
/// project reads (`ambient` names the user's) is a `Pack` origin with that
/// store, shadowed or not, the project's store asked first. A relative path
/// is the repository's. Anything else, a path outside every store included,
/// reports `Repo`: the safe default that permits ordinary composition instead
/// of one that would refuse it, since only `Pack` narrows what `use:` may
/// reach.
pub fn origin_of(repo_root: &Path, ambient: Option<&Env>, workflow_path: &Path) -> WorkflowOrigin {
    // A relative `workflow_path` (the common case — a CLI arg typed
    // as-is) is relative to `repo_root`, same as every other path this
    // module resolves; comparing it against an absolute store without
    // this would always miss.
    let absolute = if workflow_path.is_relative() {
        repo_root.join(workflow_path)
    } else {
        workflow_path.to_path_buf()
    };
    PackStores::of(repo_root, ambient)
        .in_order()
        .find_map(|store| {
            let pack = store.pack_holding(&absolute)?;
            Some(WorkflowOrigin::Pack {
                publisher: pack.publisher().clone(),
                pack_name: pack.name().clone(),
                store: store.clone(),
            })
        })
        .unwrap_or(WorkflowOrigin::Repo)
}
