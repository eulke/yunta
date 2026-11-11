//! See [`crate`]. What stands over a stage until it is answered, drawn as Surco's
//! overlay: a confirmation of an act. The palette and help join it.
//!
//! While a dialog stands, strokes go to it and not to the stage under it; answering
//! it closes it.

pub(crate) mod confirmation;

pub use confirmation::Confirmation;

/// What stands over a stage until it is answered.
#[derive(Debug, Clone)]
pub enum Dialog {
    Confirmation(Confirmation),
}
