//! The links a run's base commit holds where an agent CLI loads its
//! configuration or its instructions. A CLI reads through a link, so what a
//! link resolves to inside the checkout is protected for the reason the CLI
//! loads the link's path: the strictest reason of a glob of what it loads
//! that holds the path, or that of a tree the link stands at or above. Every
//! path the resolution walks through is held on the way, so no link put in
//! its place leads the lookup elsewhere, and so is the root of every tree a
//! CLI loads, with each directory above it. A link inside what a link leads
//! to is followed in its turn.
//!
//! The base commit is read once, when the run is born, and the links it holds
//! there travel in the manifest: every wake builds the same set from them
//! without asking git again.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Entry, Protected, ProtectedError, Stored};
use crate::glob::ScopeGlob;

/// A link at a path an agent CLI loads, and what it resolves to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoadedLink {
    /// The link, relative to the checkout.
    pub path: PathBuf,
    /// The path it resolves to, relative to the checkout, every link on the
    /// way followed.
    pub target: PathBuf,
    /// The paths the resolution walks through before the target, in the
    /// order it walks them: each directory, and each link it follows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passed: Vec<PathBuf>,
    /// Whether a CLI loads every path under the target through the link:
    /// the commit holds no file there.
    pub tree: bool,
}

/// How many links one path may pass through before it resolves to nothing:
/// the bound Linux holds a path lookup to.
const HOPS: usize = 40;

/// Every link of a commit's tree, by its path and the text it points by,
/// resolved inside the checkout: its text read from the link's directory,
/// each link on the way followed. `is_file` says whether the tree holds a file
/// at a path. A link whose text is absolute, that climbs out of the checkout
/// or that passes through more than `HOPS` links resolves to nothing, since it
/// names no path inside the checkout. So does one that names the checkout
/// itself: protecting that would leave a session nothing to write.
pub fn resolved(
    links: &BTreeMap<PathBuf, String>,
    is_file: impl Fn(&Path) -> bool,
) -> Vec<LoadedLink> {
    links
        .iter()
        .filter_map(|(path, text)| {
            let (target, passed) = resolve(path, text, links)?;
            let tree = !is_file(&target);
            Some(LoadedLink {
                path: path.clone(),
                target,
                passed,
                tree,
            })
        })
        .collect()
}

/// The checkout path `text` names from the directory of the link at `link`,
/// and the paths walked on the way. A `..` steps out of the directory a link
/// on the way resolved to, as the filesystem steps out of it.
fn resolve(
    link: &Path,
    text: &str,
    links: &BTreeMap<PathBuf, String>,
) -> Option<(PathBuf, Vec<PathBuf>)> {
    let mut at: Vec<String> = link
        .parent()
        .map(|dir| dir.iter().map(|name| name.to_string_lossy().into_owned()))
        .into_iter()
        .flatten()
        .collect();
    let mut ahead = names(text)?;
    let mut passed: Vec<PathBuf> = Vec::new();
    let mut hops = 0;
    while let Some(name) = ahead.pop_front() {
        match name.as_str() {
            "." => {}
            ".." => {
                at.pop()?;
            }
            _ => {
                at.push(name);
                let here = joined(&at);
                if let Some(next) = links.get(&here) {
                    hops += 1;
                    if hops > HOPS {
                        return None;
                    }
                    at.pop();
                    for name in names(next)?.into_iter().rev() {
                        ahead.push_front(name);
                    }
                    passed.push(here);
                } else if !ahead.is_empty() {
                    passed.push(here);
                }
            }
        }
    }
    let target = (!at.is_empty()).then(|| joined(&at))?;
    let mut way: Vec<PathBuf> = Vec::new();
    for step in passed.into_iter().filter(|step| *step != target) {
        if !way.contains(&step) {
            way.push(step);
        }
    }
    Some((target, way))
}

/// The names a link's text walks through; `None` for an absolute or an
/// empty text.
fn names(text: &str) -> Option<VecDeque<String>> {
    let names: VecDeque<String> = text
        .split('/')
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect();
    (!text.starts_with('/') && !names.is_empty()).then_some(names)
}

fn joined(names: &[String]) -> PathBuf {
    names.iter().collect()
}

impl Protected {
    /// The links of `links` a CLI loads through, as this set grows by them:
    /// each at a path a glob of what a CLI loads holds, or at or above a tree
    /// it loads, the trees a followed link leads to included.
    pub fn loaded(&self, links: Vec<LoadedLink>) -> Result<Vec<LoadedLink>, ProtectedError> {
        let (_, followed) = self.grown(&links)?;
        Ok(links
            .into_iter()
            .zip(followed)
            .filter_map(|(link, followed)| followed.then_some(link))
            .collect())
    }

    /// The set grown by what each of `links` it reaches carries. A glob the
    /// set holds already is kept once, and what a person granted the set
    /// stays lifted.
    pub fn following(self, links: &[LoadedLink]) -> Result<Protected, ProtectedError> {
        Ok(self.grown(links)?.0)
    }

    /// The set grown round by round until a round adds nothing: every link
    /// carries what it resolves to under each reason the set reaches it for
    /// so far, so a link that carry reaches follows in the next round, and a
    /// stricter reason that reaches a link later is carried too. Which of
    /// `links` the grown set follows, in their order.
    fn grown(&self, links: &[LoadedLink]) -> Result<(Protected, Vec<bool>), ProtectedError> {
        let mut set = self.clone();
        loop {
            let mut entries = set.entries.clone();
            for link in links {
                for entry in set.carried(link)? {
                    if !entries.contains(&entry) {
                        entries.push(entry);
                    }
                }
            }
            if entries.len() == set.entries.len() {
                let followed = links
                    .iter()
                    .map(|link| Ok(!set.carried(link)?.is_empty()))
                    .collect::<Result<_, ProtectedError>>()?;
                return Ok((set, followed));
            }
            set = Protected::try_from(Stored {
                entries,
                lifted: set.granted.clone(),
            })?;
        }
    }

    /// What `link` carries. When a glob of what a CLI loads holds the link's
    /// path, its target under the strictest such reason, every path under it
    /// too when it is a tree, and the paths walked on the way. When the link
    /// stands at or above a tree a CLI loads, every path under that tree's
    /// place beneath the target, and the way down to it.
    fn carried(&self, link: &LoadedLink) -> Result<Vec<Entry>, ProtectedError> {
        let loads = |entry: &&Entry| entry.reason.follows_links() && !entry.on_the_way;
        let mut carried = Vec::new();
        let holding = self
            .every
            .matches(&link.path)
            .into_iter()
            .filter_map(|index| self.entries.get(index))
            .filter(loads)
            .map(|entry| entry.reason)
            .min();
        if let Some(reason) = holding {
            carried.push(match link.tree {
                true => Entry::of(reason, ScopeGlob::under(&link.target)?),
                false => Entry::of(reason, ScopeGlob::exact(&link.target)?),
            });
            let way = link.passed.iter().chain(link.tree.then_some(&link.target));
            for step in way {
                carried.push(Entry::on_the_way(reason, step)?);
            }
        }
        for entry in self.entries.iter().filter(loads) {
            let Some(below) = entry
                .glob
                .whole_tree()
                .and_then(|tree| Path::new(tree).strip_prefix(&link.path).ok())
            else {
                continue;
            };
            let down = way_down(&link.target, below);
            let reached = down.last().unwrap_or(&link.target);
            carried.push(Entry::of(entry.reason, ScopeGlob::under(reached)?));
            for step in link.passed.iter().chain(&down) {
                carried.push(Entry::on_the_way(entry.reason, step)?);
            }
        }
        Ok(carried)
    }
}

/// The directories a lookup of a path under one of `trees` passes: each whole
/// tree's root and every directory above it, once each.
pub(super) fn roots_of(trees: &[ScopeGlob]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for tree in trees.iter().filter_map(ScopeGlob::whole_tree) {
        for root in Path::new(tree).ancestors() {
            if !root.as_os_str().is_empty() && !roots.iter().any(|known| known == root) {
                roots.push(root.to_path_buf());
            }
        }
    }
    roots
}

/// The paths from `target` down to `target/below`, both included.
fn way_down(target: &Path, below: &Path) -> Vec<PathBuf> {
    let mut way = vec![target.to_path_buf()];
    let mut at = target.to_path_buf();
    for name in below {
        at.push(name);
        way.push(at.clone());
    }
    way
}

#[cfg(test)]
mod tests;
