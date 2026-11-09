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
pub use listed::{Listed, VendoredPack, VendoredTree};
pub use new_pack::{NewPack, NewPackCheck};
pub use removed::Removed;
pub use updated::Updated;
