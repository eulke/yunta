//! See [`super`]. The packs a project reads, store by store: `store_packs`
//! lists the pack directories of each store with shadowing applied, and
//! `first_answer` asks the stores in order until one answers. Every reader
//! of packs reads through one of them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use yunta_core::{Env, PackManifest, PackScope, PackStore, PackStores, Publisher};

use super::{CatalogError, DeclaredPath, WorkflowOrigin, PACK_MANIFEST};

/// A pack as it sits in a store: where, and what its manifest declares.
#[derive(Debug, Clone)]
pub struct StoredPack {
    pub store: PackStore,
    pub dir: PathBuf,
    pub manifest: PackManifest,
}

impl StoredPack {
    /// The origin of a workflow this pack declares.
    pub fn origin(&self) -> WorkflowOrigin {
        WorkflowOrigin::Pack {
            publisher: self.manifest.publisher.clone(),
            pack_name: self.manifest.name.clone(),
            store: self.store.clone(),
        }
    }

    /// `path`, as this pack's manifest declares it.
    fn declared(&self, path: String) -> DeclaredPath {
        DeclaredPath::of_pack(self.dir.clone(), &self.manifest, path)
    }
}

/// Every pack a project reads under a publisher: the ones whose `pack.yaml`
/// parsed, the project's before the user's, and the ones that did not. A
/// broken pack is named (as a [`CatalogError::Unreadable`] or
/// [`CatalogError::Malformed`]), never silently dropped from the catalog a
/// human is looking at.
pub struct PublisherPacks {
    pub installed: Vec<StoredPack>,
    pub broken: Vec<CatalogError>,
}

impl PublisherPacks {
    /// Every workflow these packs declare, by the basename a
    /// `publisher/name` reference looks it up by, with its file: each name
    /// listed from the first store whose packs declare it, as a lookup
    /// answers it, and from every pack of that store that declares it.
    pub fn workflows(&self) -> Vec<(String, PathBuf)> {
        let mut listed = Vec::new();
        let mut answered: HashSet<String> = HashSet::new();
        for in_store in self.installed.chunk_by(|a, b| a.store == b.store) {
            let declared: Vec<(String, PathBuf)> = in_store
                .iter()
                .flat_map(|pack| {
                    pack.manifest.contents.workflows.iter().filter_map(|file| {
                        let stem = Path::new(file).file_stem()?.to_str()?;
                        Some((stem.to_string(), pack.dir.join(file)))
                    })
                })
                .collect();
            listed.extend(
                declared
                    .iter()
                    .filter(|(name, _)| !answered.contains(name))
                    .cloned(),
            );
            answered.extend(declared.into_iter().map(|(name, _)| name));
        }
        listed
    }
}

/// What a project's packs declare as knowledge, unjoined: the paths the
/// project's packs declare, and those the user's packs it does not shadow
/// declare, each in the order the packs are listed.
#[derive(Default)]
pub(crate) struct DeclaredKnowledge {
    pub(crate) project: Vec<DeclaredPath>,
    pub(crate) user: Vec<DeclaredPath>,
}

/// For `publisher`, the pack directories each store holds, in the order the
/// stores are read. A directory is a pack when it holds a `pack.yaml`, read
/// or not. The user's list leaves out each pack the project's store holds
/// too, by `publisher/name`: the project's copy shadows it whole, and a
/// project whose own store is the user's reads each pack once. A store's
/// directory is listed only when the walk reaches it.
fn store_packs<'s>(
    stores: &'s PackStores,
    publisher: &'s Publisher,
) -> impl Iterator<Item = (&'s PackStore, Vec<PathBuf>)> + 's {
    stores.in_order().map(move |store| {
        let mut dirs = pack_dirs(&store.publisher_dir(publisher));
        if store.scope() == PackScope::User {
            let project = stores.project().publisher_dir(publisher);
            dirs.retain(|dir| {
                dir.file_name()
                    .is_none_or(|name| !holds_pack(&project.join(name)))
            });
        }
        (store, dirs)
    })
}

/// What the first store that answers gives: `answer` reads one store's pack
/// directories for what they declare under the name looked up, and the
/// first store whose answer is not empty answers. The stores after it are
/// never read.
fn first_answer<T, E>(
    stores: &PackStores,
    publisher: &Publisher,
    mut answer: impl FnMut(&PackStore, Vec<PathBuf>) -> Result<Vec<T>, E>,
) -> Result<Vec<T>, E> {
    for (store, dirs) in store_packs(stores, publisher) {
        let found = answer(store, dirs)?;
        if !found.is_empty() {
            return Ok(found);
        }
    }
    Ok(Vec::new())
}

/// The packs that declare a workflow file whose basename is `workflow`,
/// each with the path it declares, from the first store whose packs declare
/// one. A pack whose manifest does not read, in a store the lookup reaches,
/// is the error: it may be the very pack that declares `workflow`, so passing
/// it by would answer `NotFound` and hide the real fault.
pub(super) fn declaring_packs(
    stores: &PackStores,
    publisher: &Publisher,
    workflow: &str,
) -> Result<Vec<(StoredPack, DeclaredPath)>, CatalogError> {
    first_answer(stores, publisher, |store, dirs| {
        let mut found = Vec::new();
        for dir in dirs {
            let pack = stored(store, dir)?;
            for file in &pack.manifest.contents.workflows {
                if Path::new(file).file_stem().and_then(|s| s.to_str()) == Some(workflow) {
                    found.push((pack.clone(), pack.declared(file.clone())));
                }
            }
        }
        Ok(found)
    })
}

/// The one path a publisher's packs declare that `names` picks out of a
/// manifest, from the first store whose packs declare one: an agent, a
/// skill. `None` when no pack declares it, or when two packs of that store
/// do, since nothing orders them. A pack whose manifest does not read
/// declares nothing here.
pub(crate) fn pack_declaring(
    repo_root: &Path,
    ambient: Option<&Env>,
    publisher: &Publisher,
    names: impl Fn(&PackManifest) -> Vec<String>,
) -> Option<DeclaredPath> {
    let stores = PackStores::of(repo_root, ambient);
    let Ok(found) =
        first_answer::<_, std::convert::Infallible>(&stores, publisher, |store, dirs| {
            Ok(dirs
                .into_iter()
                .filter_map(|dir| stored(store, dir).ok())
                .flat_map(|pack| {
                    let paths = names(&pack.manifest);
                    paths.into_iter().map(move |path| pack.declared(path))
                })
                .collect())
        });
    let [one] = <[DeclaredPath; 1]>::try_from(found).ok()?;
    Some(one)
}

/// Every pack a project reads under `publisher`, from each store in order,
/// the user's without those the project's shadows. `ambient` is the
/// environment the user's store is found in; `None` reads only the
/// project's. `yunta list`'s catalog and `doctor` walk every publisher and
/// read each through this.
pub fn packs_for_publisher(
    repo_root: &Path,
    ambient: Option<&Env>,
    publisher: &Publisher,
) -> PublisherPacks {
    let stores = PackStores::of(repo_root, ambient);
    let mut packs = PublisherPacks {
        installed: Vec::new(),
        broken: Vec::new(),
    };
    for (store, dirs) in store_packs(&stores, publisher) {
        for dir in dirs {
            match stored(store, dir) {
                Ok(pack) => packs.installed.push(pack),
                Err(broken) => packs.broken.push(broken),
            }
        }
    }
    packs
}

/// Every path an installed pack declares as knowledge, unjoined, the
/// project's packs apart from the user's.
pub(crate) fn declared_knowledge(repo_root: &Path, ambient: Option<&Env>) -> DeclaredKnowledge {
    let mut declared = DeclaredKnowledge::default();
    for publisher in installed_publishers(repo_root, ambient) {
        for pack in packs_for_publisher(repo_root, ambient, &publisher).installed {
            let paths = pack.manifest.contents.knowledge.iter();
            let paths = paths.map(|path| pack.declared(path.clone()));
            match pack.store.scope() {
                PackScope::Project => declared.project.extend(paths),
                PackScope::User => declared.user.extend(paths),
            }
        }
    }
    declared
}

/// Every publisher a project reads packs of: the directories under each
/// store's packs, sorted, each named once.
pub fn installed_publishers(repo_root: &Path, ambient: Option<&Env>) -> Vec<Publisher> {
    let stores = PackStores::of(repo_root, ambient);
    let mut publishers: Vec<Publisher> = stores
        .in_order()
        .flat_map(|store| {
            subdirectories(&store.packs_dir())
                .into_iter()
                .filter_map(|dir| dir.file_name()?.to_str()?.parse().ok())
        })
        .collect();
    publishers.sort();
    publishers.dedup();
    publishers
}

/// The packs under a publisher's directory, sorted: each directory that
/// holds a `pack.yaml`. A directory with none is not a pack.
fn pack_dirs(publisher_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = subdirectories(publisher_dir);
    dirs.retain(|dir| holds_pack(dir));
    dirs.sort();
    dirs
}

/// Whether `dir` holds a `pack.yaml`, read or not.
fn holds_pack(dir: &Path) -> bool {
    dir.join(PACK_MANIFEST).exists()
}

/// The directories directly under `dir`; none when it cannot be read.
fn subdirectories(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

/// The pack at `dir` in `store`, its manifest read: a manifest that cannot
/// be read or parsed is the error that names it.
fn stored(store: &PackStore, dir: PathBuf) -> Result<StoredPack, CatalogError> {
    let path = dir.join(PACK_MANIFEST);
    let text = std::fs::read_to_string(&path).map_err(|e| CatalogError::Unreadable {
        path: path.clone(),
        detail: e.to_string(),
    })?;
    let manifest = PackManifest::read(&text).map_err(|e| CatalogError::Malformed {
        path,
        detail: e.to_string(),
    })?;
    Ok(StoredPack {
        store: store.clone(),
        dir,
        manifest,
    })
}
