//! A pack's pages: the inventory a person reads before a pack is vendored
//! or updated, or of an installed pack audited; what an install, an update,
//! a removal and a listing answer with; and what `pack new` wrote. Each is
//! filled with what its command read — the audit, the lock, the vendored
//! tree, the pack's own cases — and reads nothing itself.

mod cases;
mod installed;
mod inventory;
mod listed;
mod new_pack;
mod removed;
mod updated;

pub use cases::Cases;
pub use installed::{Installed, Need, Needs};
pub use inventory::Inventory;
pub use listed::{Listed, PointedPacks, VendoredPack, VendoredTree};
pub use new_pack::{NewPack, NewPackCheck};
pub use removed::Removed;
pub use updated::Updated;

use std::path::Path;

/// What a symbolic link found in a pack's own tree is, wherever the tree is read, at its audit
/// or against its lock: something a pack never ships. Not a link a pack's source holds as it is
/// fetched, which an add refuses before anything of it is read.
pub fn link_in_its_tree(link: &Path) -> String {
    format!(
        "`{}` is a symbolic link in the pack's tree, which a pack never ships",
        link.display()
    )
}
