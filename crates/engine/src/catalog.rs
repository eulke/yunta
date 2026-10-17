//! Namespaced resolution: a bare name always means
//! the repo's own `.yunta/workflows/`; a `publisher/name` reference
//! falls through to that publisher's vendored packs *only when the repo
//! has nothing by that literal name* — a local workflow with the same
//! name shadows the one from a pack. Within a publisher, `name` addresses a
//! **workflow's own file basename** as declared in some installed
//! pack's `contents.workflows` — not the pack's own `name` field, which
//! is packaging/versioning metadata (`review-pack`, `1.2.0`), distinct
//! from the workflow files it ships (`review.yaml`). A publisher's
//! installed packs share one flat namespace; two packs from the same
//! publisher declaring the same workflow basename is ambiguous, not
//! silently resolved by install order.

use std::path::{Path, PathBuf};

use yunta_core::{PackManifest, PackName, Publisher};

#[derive(Debug, Clone, thiserror::Error)]
pub enum CatalogError {
    #[error(
        "no workflow `{name}` — not in the repo catalog ({repo_path}), and no pack under \
         publisher `{publisher}` declares a workflow named `{workflow}`"
    )]
    NotFound {
        name: String,
        repo_path: PathBuf,
        publisher: String,
        workflow: String,
    },
    #[error("no workflow `{name}` — not in the repo catalog ({repo_path})")]
    NotFoundNoPacksDir { name: String, repo_path: PathBuf },
    #[error(
        "`{name}` is ambiguous — {count} packs under publisher `{publisher}` each declare a \
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
        "`{name}` is not a workflow reference — a reference is a bare name or `publisher/name`, \
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

/// Every pack under a publisher: the ones whose `pack.yaml` parsed, and
/// the ones that did not. A broken pack is named (as a
/// [`CatalogError::Unreadable`] or [`CatalogError::Malformed`]), never
/// silently dropped from the catalog a human is looking at.
pub struct PublisherPacks {
    pub installed: Vec<(PathBuf, PackManifest)>,
    pub broken: Vec<CatalogError>,
}

/// Where a resolved workflow file actually came from — `check`'s
/// cross-pack composition rule (cross-pack references aren't supported)
/// needs this to tell "still inside the same pack" from
/// "left it", which a bare [`PathBuf`] can't say on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowOrigin {
    Repo,
    Pack {
        publisher: Publisher,
        pack_name: PackName,
    },
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
/// first, packs second (packs are the bottom
/// layer, never shadowing something the repo already names). A pack's
/// declared path that leaves the pack is refused, naming the pack and the
/// path, so no reader loads a file outside the pack that declares it.
pub fn resolve_workflow(repo_root: &Path, name: &str) -> Result<ResolvedWorkflow, CatalogError> {
    Ok(match name_workflow(repo_root, name)? {
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
pub(crate) fn name_workflow(repo_root: &Path, name: &str) -> Result<Named, CatalogError> {
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

    let publisher_dir = repo_root.join(".yunta/packs").join(publisher);
    let Some(candidates) = declaring_packs(&publisher_dir, workflow)? else {
        return Err(CatalogError::NotFound {
            name: name.to_string(),
            repo_path,
            publisher: publisher.to_string(),
            workflow: workflow.to_string(),
        });
    };
    let (pack_name, declared) = choose(name, repo_path, publisher, workflow, candidates)?;
    Ok(Named::Pack {
        declared,
        origin: WorkflowOrigin::Pack {
            publisher: publisher_id,
            pack_name,
        },
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
    mut candidates: Vec<(PackName, DeclaredPath)>,
) -> Result<(PackName, DeclaredPath), CatalogError> {
    if candidates.len() > 1 {
        return Err(CatalogError::Ambiguous {
            name: name.to_string(),
            publisher: publisher.to_string(),
            workflow: workflow.to_string(),
            count: candidates.len(),
            candidates: candidates
                .iter()
                .map(|(pack_name, _)| pack_name.as_str())
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

/// The installed packs under a publisher's directory that declare a
/// workflow file whose basename is `workflow`, each with the path it
/// declares. `None` when the publisher has no readable directory.
fn declaring_packs(
    publisher_dir: &Path,
    workflow: &str,
) -> Result<Option<Vec<(PackName, DeclaredPath)>>, CatalogError> {
    let Ok(entries) = std::fs::read_dir(publisher_dir) else {
        return Ok(None);
    };
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let pack_dir = entry.path();
        if !pack_dir.is_dir() {
            continue;
        }
        // A broken pack in the search path is named, never skipped: it
        // may be the very pack that declares `workflow`, so passing it by
        // would resolve to `NotFound` and hide the real fault. A
        // directory with no `pack.yaml` at all is not a pack, so that one
        // case is skipped.
        let manifest_path = pack_dir.join("pack.yaml");
        let manifest_text = match std::fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(CatalogError::Unreadable {
                    path: manifest_path,
                    detail: e.to_string(),
                })
            }
        };
        let manifest = yunta_core::yaml::parse::<PackManifest>(&manifest_text).map_err(|e| {
            CatalogError::Malformed {
                path: manifest_path,
                detail: e.to_string(),
            }
        })?;
        for declared in &manifest.contents.workflows {
            let stem_matches = Path::new(declared)
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| stem == workflow);
            if stem_matches {
                let found = DeclaredPath::of_pack(pack_dir.clone(), &manifest, declared.clone());
                candidates.push((manifest.name.clone(), found));
            }
        }
    }
    Ok(Some(candidates))
}

/// Every pack vendored under `.yunta/packs/<publisher>/` whose manifest
/// parses — `yunta list`'s own catalog view walks every publisher
/// directory and calls this per publisher, same resolver both surfaces
/// share.
pub fn packs_for_publisher(repo_root: &Path, publisher: &Publisher) -> PublisherPacks {
    let publisher_dir = repo_root.join(".yunta/packs").join(publisher.as_str());
    let mut packs = PublisherPacks {
        installed: Vec::new(),
        broken: Vec::new(),
    };
    let Ok(entries) = std::fs::read_dir(&publisher_dir) else {
        return packs;
    };
    for entry in entries.flatten() {
        let pack_dir = entry.path();
        if !pack_dir.is_dir() {
            continue;
        }
        // A directory with no `pack.yaml` is not a pack; one whose
        // manifest exists but cannot be read or parsed is a broken pack,
        // named — never dropped from what `yunta list`/`doctor` show.
        let manifest_path = pack_dir.join("pack.yaml");
        let manifest_text = match std::fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                packs.broken.push(CatalogError::Unreadable {
                    path: manifest_path,
                    detail: e.to_string(),
                });
                continue;
            }
        };
        match yunta_core::yaml::parse::<PackManifest>(&manifest_text) {
            Ok(manifest) => packs.installed.push((pack_dir, manifest)),
            Err(e) => packs.broken.push(CatalogError::Malformed {
                path: manifest_path,
                detail: e.to_string(),
            }),
        }
    }
    packs
}

/// Every path an installed pack declares as knowledge, unjoined, in the
/// order the packs are listed.
pub(crate) fn declared_knowledge(repo_root: &Path) -> Vec<DeclaredPath> {
    let mut declared = Vec::new();
    for publisher in installed_publishers(repo_root) {
        for (pack_dir, manifest) in packs_for_publisher(repo_root, &publisher).installed {
            for path in &manifest.contents.knowledge {
                declared.push(DeclaredPath::of_pack(
                    pack_dir.clone(),
                    &manifest,
                    path.clone(),
                ));
            }
        }
    }
    declared
}

/// Where `workflow_path` actually lives, relative to `repo_root` — the
/// origin `check_workflow_refs`'s caller must supply for its *top-level*
/// workflow, since that function only ever sees a parsed
/// [`Workflow`](yunta_core::Workflow) value with no memory of the file it
/// came from. `.yunta/packs/
/// <publisher>/<pack>/...` reports `Pack`; anything else (including a
/// path outside `repo_root` entirely) reports `Repo` — the safe default
/// that permits ordinary composition instead of one that would refuse
/// it, since only `Pack` narrows what `use:` may reach.
pub fn origin_of(repo_root: &Path, workflow_path: &Path) -> WorkflowOrigin {
    let packs_root = repo_root.join(".yunta/packs");
    // A relative `workflow_path` (the common case — a CLI arg typed
    // as-is) is relative to `repo_root`, same as every other path this
    // module resolves; comparing it against an absolute `packs_root`
    // without this would always miss.
    let absolute = if workflow_path.is_relative() {
        repo_root.join(workflow_path)
    } else {
        workflow_path.to_path_buf()
    };
    let Ok(relative) = absolute.strip_prefix(&packs_root) else {
        return WorkflowOrigin::Repo;
    };
    let mut components = relative.components();
    let (Some(publisher), Some(pack_name)) = (components.next(), components.next()) else {
        return WorkflowOrigin::Repo;
    };
    // A directory pair that is not a `publisher/name` was never vendored
    // by `pack add`; it is not a pack origin.
    let parsed = (
        publisher.as_os_str().to_str().and_then(|s| s.parse().ok()),
        pack_name.as_os_str().to_str().and_then(|s| s.parse().ok()),
    );
    match parsed {
        (Some(publisher), Some(pack_name)) => WorkflowOrigin::Pack {
            publisher,
            pack_name,
        },
        _ => WorkflowOrigin::Repo,
    }
}

/// Every installed publisher — top-level names under `.yunta/packs/`.
pub fn installed_publishers(repo_root: &Path) -> Vec<Publisher> {
    let packs_root = repo_root.join(".yunta/packs");
    let Ok(entries) = std::fs::read_dir(&packs_root) else {
        return Vec::new();
    };
    let mut publishers: Vec<Publisher> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse().ok())
        })
        .collect();
    publishers.sort();
    publishers
}
