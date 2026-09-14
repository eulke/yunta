//! A node asking to write outside the scope it declared, and the answer
//! it got.

pub mod kinds;
pub mod ledger;
pub mod payloads;

pub use kinds::ScopeEvent;
pub use ledger::*;
pub use payloads::*;
