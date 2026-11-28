//! See [`super`]. The pack a setup installs ([`SetupPack`]): the Yunta repository's
//! reference pack, `yunta/fragua`, from the directory the repository holds it in, at
//! the tag that marks this binary's version, so a setup offers the workflows released
//! with the binary that sets the repository up. The repository's URL is the package
//! definition's, and the source is written here once, in the form a person gives
//! `yunta pack add`.
//!
//! The version is the package definition's too, so a binary built between two
//! releases asks for the tag of the version it was built at: the last release's until
//! the version moves, then one that stands only once that release is tagged. Fetching
//! it reaches the network, for the tag alone, at depth one, never the repository's whole
//! history; a pack fetched is cloned into a directory of its own, and nothing of it
//! reaches the repository until it is installed. The setup knows its pack's
//! name before any clone, so a repository that vendors it already is read first, locally,
//! and nothing is fetched ([`SetupPack::fetched`]).

use yunta_core::{PackName, PackRef, Publisher};

use crate::{Error, FetchedPack, Stop, Workspace};

/// The Yunta repository's URL, as the package definition names it.
const YUNTA_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The tag the Yunta repository marks this binary's version with.
const VERSION_TAG: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Where the reference pack stands in the Yunta repository.
const DIRECTORY: &str = "packs/fragua";

/// The pack a setup installs, and where it is fetched from.
///
/// Not a pack a person names: the one a setup offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupPack {
    pack: PackRef,
    source: String,
}

impl SetupPack {
    /// `yunta/fragua`, from the Yunta repository's `packs/fragua` at this binary's
    /// version tag (`v` and its version).
    pub fn of_this_binary() -> SetupPack {
        let pack = PackRef::new(
            Publisher::from_static("yunta"),
            PackName::from_static("fragua"),
        );
        SetupPack {
            pack,
            source: format!("{YUNTA_REPOSITORY}//{DIRECTORY}@{VERSION_TAG}"),
        }
    }

    /// The pack, as its manifest names it.
    pub fn pack(&self) -> &PackRef {
        &self.pack
    }

    /// Where it is fetched from, `<repository>//<directory>@<ref>`: what
    /// [`Packs::fetch`](crate::Packs::fetch) takes.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The pack fetched to install it into `workspace`'s repository, as
    /// [`Packs::fetch`](crate::Packs::fetch) fetches it, its tag alone cloned, at depth one;
    /// none where the repository vendors it already, which is read first, locally, so
    /// nothing is cloned and no network asked. Not `Packs::fetch` alone, which clones first,
    /// since a source does not say its pack's name, and clones the whole repository, since a
    /// ref a person names can be a commit.
    pub async fn fetched(
        &self,
        workspace: &Workspace,
        stop: &Stop,
    ) -> Result<Option<FetchedPack>, Error> {
        let packs = workspace.packs();
        if packs.vendors(&self.pack) {
            return Ok(None);
        }
        packs.fetch_tagged(&self.source, stop).await.map(Some)
    }
}
