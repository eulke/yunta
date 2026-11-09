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
//! - [`Run`]: a run, as `status`, its closing or a browser reads it ([`RunScope`]),
//!   from where it lives ([`Places`]); [`RunInABrowser`]: the page `open` writes,
//!   with each document a decision is about before it.
//! - [`Node`]: one of a run's nodes, as `status --node` shows it.
//! - [`LeftRunning`]: a run a person stopped following, which goes on.
//! - [`Field`]: the runs of a repository or a machine, each a [`FieldRow`] in
//!   its [`FieldGroup`], and those that do not read back ([`UnreadableRun`]).
//! - [`Stats`]: what a run spent and where its time went; [`History`]: what a
//!   workflow's past runs say, and replay's `VerificationFindings`, how its
//!   checks performed, a page of its own.
//! - [`Doctor`]: whether this machine is ready; [`Verified`]: whether a run's
//!   evidence is intact.
//! - [`Cancelled`] ([`CancelState`]), [`Closed`], [`Opened`]: what `cancel`,
//!   `close` and `open` answer; [`GateAnswered`] ([`Resolution`]), [`Collected`] and
//!   [`Review`]: what `resolve-gate`, `gc` and `review` answer.
//! - [`Catalog`]: the workflows a repository offers, each an [`OfferedWorkflow`];
//!   [`Signpost`]: what `yunta` alone shows where it has no field to show.
//! - [`Schema`]: every kind of document Yunta reads, or every key a workflow
//!   reads.
//! - [`Checked`]: the verdict on a workflow file, with what `check` diagnosed
//!   ([`Diagnosis`], [`Unrunnable`]); [`NewWorkflow`]: what `new` wrote, and the
//!   verdict on it; [`Initialized`]: what `init` wrote and detected.
//! - [`Tested`] and [`TestedCase`]: what `test` says as its cases run, each a
//!   [`RanCase`] and how its run ended ([`CaseRun`]).
//! - [`pack`]: a pack's pages: its inventory, and what `pack add`, `update`,
//!   `remove`, `list` and `new` answer.
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

mod answered;
mod cancelled;
mod catalog;
mod checked;
mod checklist;
mod closed;
mod collected;
pub mod desk;
mod doctor;
mod field;
pub(crate) mod findings;
mod gate;
mod initialized;
mod left_running;
mod new_workflow;
mod node;
mod opened;
pub mod pack;
pub(crate) mod plan;
mod receipt;
mod review;
mod run;
mod schema;
pub(crate) mod shown;
mod signpost;
pub(crate) mod spec;
mod stats;
mod tested;
mod verified;

pub use answered::{GateAnswered, Resolution};
pub use cancelled::{CancelState, Cancelled};
pub use catalog::{Catalog, OfferedWorkflow, ReadableWorkflow, UnreadableWorkflow};
pub use checked::{Checked, Diagnosis, Unrunnable};
pub use closed::Closed;
pub use collected::Collected;
pub use doctor::Doctor;
pub use field::{Field, FieldGroup, FieldRow, RunUnread, UnreadableRun};
pub use findings::Findings;
pub use gate::Gate;
pub use initialized::Initialized;
pub use left_running::LeftRunning;
pub use new_workflow::NewWorkflow;
pub use node::Node;
pub use opened::Opened;
pub use plan::{Form, Plan};
pub use receipt::Receipt;
pub use review::Review;
pub use run::{Places, Run, RunInABrowser, RunScope};
pub use schema::Schema;
pub use shown::Shown;
pub use signpost::Signpost;
pub use spec::Spec;
pub use stats::{History, Stats};
pub use tested::{CaseRun, RanCase, Tested, TestedCase};
pub use verified::Verified;

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
