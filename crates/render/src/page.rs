//! Every page Yunta shows a person takes one form: a value its caller fills
//! with what it read, drawn into one document by [`Page::document`].
//!
//! A page reads nothing: the disk, the clock and the processes are its
//! caller's to read, and what they gave are the page's fields. So the CLI
//! prints a page on a terminal, writes it for a browser or hands it to a
//! forge, and a terminal application or a desktop draws the same value on a
//! surface of its own, through the one trait.
//!
//! Each page this crate holds is a module of this one, named by what it
//! shows:
//!
//! - [`Receipt`]: a run's receipt, in a file, a pull request or a terminal.
//! - [`Findings`]: what a review found, or the run's findings.
//! - [`Gate`]: a gate's decision, above what answers it, as the place it
//!   is drawn in says.
//! - [`Shown`]: a document an escalation shows, in the [`Form`] it is read in:
//!   a [`Plan`], a [`Spec`], findings, or text.
//!
//! A page that holds another draws it as a page, with its own look.
//!
//! [`desk`] is no page: it is how a terminal reads a plan, a spot at a time.

use surco::doc::Doc;
use surco::look::Look;

pub mod desk;
pub(crate) mod findings;
mod gate;
pub(crate) mod plan;
mod receipt;
pub(crate) mod shown;
pub(crate) mod spec;

pub use findings::Findings;
pub use gate::Gate;
pub use plan::{Form, Plan};
pub use receipt::Receipt;
pub use shown::Shown;
pub use spec::Spec;

/// A page a person reads: a value its caller fills with what it read, drawn
/// into one document for a surface that looks like `look`.
pub trait Page {
    /// The page as one document.
    fn document(&self, look: &Look) -> Doc;
}

/// A page lent is the page: a command hands its answer's page by reference,
/// and whoever draws it needs no copy.
impl<P: Page + ?Sized> Page for &P {
    fn document(&self, look: &Look) -> Doc {
        (**self).document(look)
    }
}
