//! An agent session: the CLI it opened on, what it said while it ran,
//! and a capability the adapter did not have.

pub mod kinds;
pub mod ledger;
pub mod payloads;

pub use kinds::SessionEvent;
pub use ledger::*;
pub use payloads::*;
