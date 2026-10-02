//! A document an escalation shows, as the person deciding reads it.
//!
//! What they decide on is what the run holds, so the words come from the
//! document itself, cut to the width they read at, and the path under
//! them is where its file sits, in full. A plan is read the way it is
//! reviewed: what it changes and why, the shapes it creates, what it
//! risks and leaves out, the order its tasks run in, then task by task
//! what each does, what it touches and what proves it done — in words;
//! the commands that prove it stay in the whole plan, one open away. A
//! diagram has no room on a terminal either, so it is named here and
//! drawn there. A spec is read task by task, with its tests' files
//! whole; what a review found, the most severe first; the run's findings
//! the same way, each with the node that found it and how others
//! answered it.

use yunta_core::events::ArtifactId;
use yunta_core::shown::{ShownContent, ShownDocument};

use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};
pub use crate::plan::{Form, CODE_SHOWN};

/// What `document` says, for the run whose handle is `run`, in `form`.
pub fn document(document: &ShownDocument, run: &str, form: Form) -> Doc<'static> {
    let of = document
        .shown
        .producer
        .as_ref()
        .map(|node| format!(" of `{node}`"))
        .unwrap_or_default();
    match &document.content {
        ShownContent::Tasks(review) => crate::plan::document(review, &of, run, form),
        ShownContent::Spec(file) => crate::spec::document(file, &of, run, form),
        ShownContent::Findings(file) => crate::findings::document(file, &of),
        ShownContent::RunFindings(view) => crate::findings::run_document(view),
        ShownContent::Text(text) => Doc::new()
            .with(Block::Title(Line::new().push(
                Tone::Strong,
                match &document.shown.artifact {
                    ArtifactId::Interpreted { kind } => format!("the {kind} document{of}"),
                    ArtifactId::Opaque { name } => format!("{name}{of}"),
                },
            )))
            .with(Block::Markdown(text.clone())),
    }
}
