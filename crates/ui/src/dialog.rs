//! See [`crate`]. What stands over a stage until it is answered, drawn as Surco's
//! overlay: a confirmation of an act, the palette of every act from where the person
//! stands, or help, the keys there.
//!
//! While a dialog stands, strokes go to it and not to the stage under it; answering
//! it closes it.

pub(crate) mod confirmation;
pub(crate) mod help;
pub(crate) mod palette;

pub use confirmation::Confirmation;
pub use help::Help;
pub use palette::Palette;

/// What stands over a stage until it is answered.
#[derive(Debug, Clone)]
pub enum Dialog {
    Confirmation(Confirmation),
    Palette(Palette),
    Help(Help),
}
