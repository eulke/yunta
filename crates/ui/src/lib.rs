//! Yunta's user interface, apart from the medium that shows it: what a person
//! has open, what each stroke and each piece of news does to it, the work it
//! asks of Yunta as data, and each moment as a Surco `Screen`. The terminal
//! application and the desktop are surfaces over it: each reads its own input
//! and hands it in, carries out the work it asks for, and lays out the screen
//! it returns, deciding nothing the other would decide again.
//!
//! It writes no sentence, which is `yunta-render`'s, and the library names no
//! terminal, no window, no engine, no store and no adapter: every operation goes
//! through `yunta-api`. `tests/no_terminal_in_the_interface.rs` holds the
//! second half; a test may break a run's log through the store's test feature.
//!
//! Its map:
//!
//! - The application as values: `state`, the [`State`] an [`Input`] changes,
//!   which says what it asks for as [`Request`]s and is read as a screen; `stage`,
//!   the [`Stage`] a person is at; `dialog`, the [`Dialog`] standing over it until it
//!   is answered; `operation`, the [`Operation`]s a stage asks of
//!   Yunta under a [`Ticket`] (`ticket`); `outcome` and `news`, what that work comes
//!   to as an [`Outcome`] and hands back while it lasts as [`News`]; `input` and
//!   `request`.
//! - `performer`, the [`Performer`] a surface hands each request to, which carries
//!   the work out through `yunta-api` and hands back what it says and comes to;
//!   `remark`, what a remark the API makes while it works says, and how a person
//!   meets it, alike on every interface.
//! - `prompt`, the prompts a person answers stroke by stroke, each the
//!   [`Answerable`](prompt::Answerable) a surface holds, the [`Stroke`](prompt::Stroke)
//!   it is handed and the [`Press`](prompt::Press) it answers with.
//! - `scroll`, how a region taller than what a surface shows of it scrolls by the
//!   keys, held by a desk's pane and every stage that shows a page.

mod dialog;
mod input;
mod news;
mod operation;
mod outcome;
mod performer;
pub mod prompt;
pub mod remark;
mod request;
mod scroll;
mod stage;
mod state;
mod ticket;

pub use dialog::{Confirmation, Dialog};
pub use input::Input;
pub use news::News;
pub use operation::{
    EvidenceOperation, Operation, OperationKind, PackOperation, WorkflowOperation,
};
pub use outcome::{
    EvidenceOutcome, NewWorkflowWritten, Outcome, PackFetched, PackInstalled, PackOutcome,
    PackScaffolded, WorkflowOutcome,
};
pub use performer::Performer;
pub use request::Request;
pub use stage::{
    CatalogStage, DecisionStage, DoctorStage, Evidence, EvidenceStage, FieldStage, FirstRunStage,
    GraphStage, RunStage, SchemaStage, SetupStage, Stage, StartStage, StatsStage, UpkeepStage,
};
pub use state::State;
pub use ticket::Ticket;
