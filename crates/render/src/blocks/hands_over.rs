//! What a node hands over: each artifact its workflow declares it produces, a row each, with
//! where it stands on the attempt that is open. An attempt hands its documents over again,
//! so what an attempt before it handed over is not yet handed over by this one.
//!
//! A document the engine accepted that cannot be read here is said in its own errors, as
//! its reader worded why: its path and how many errors, then each error. Those lines are
//! the reader's, the same a decision that shows the document says; this block words none
//! of them, and wraps each under the column where it opens.

use surco::doc::Block;
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::width::{cell_width, wrap_unbroken};
use yunta_core::events::ArtifactId;
use yunta_core::{ArtifactSpec, NodeId};
use yunta_replay::RunState;

/// The words over what a node hands over.
const HEADING: &str = "what it hands over";

/// The cells that part an artifact's name from where it stands: two, so the gap reads apart
/// from the one space between the words of what it says.
const APART: usize = 2;

/// What a node hands over: each artifact its workflow declares it produces, in the order
/// declared, with where it stands on the node's open attempt. Not the artifacts' ledger's
/// `HandedOver`, how many times a node submitted a document and how many the engine refused.
pub struct HandsOver {
    documents: Vec<(String, DocumentStands)>,
}

/// Where a document a node declares stands on the attempt that is open.
enum DocumentStands {
    /// The attempt has not handed it over.
    NotYet,
    /// The engine read it and accepted it.
    Accepted,
    /// The engine accepted it, and it cannot be read here, for `why`, as its reader worded
    /// it: it fails today's shape, or its bytes cannot be read.
    Unread { why: String },
}

impl HandsOver {
    /// What `node` hands over of the artifacts it `declared`, as the artifacts' ledger of the
    /// run in `state` holds them since the node's latest attempt started; one accepted that its
    /// reader could not read is unread, with the why `unread` gives it.
    pub fn of(
        node: &NodeId,
        declared: &[ArtifactSpec],
        state: &RunState,
        unread: &[(ArtifactId, String)],
    ) -> HandsOver {
        let since = state.nodes.get(node).and_then(|record| record.last_started);
        let documents = declared
            .iter()
            .map(|spec| {
                let artifact = ArtifactId::from(spec);
                let accepted = state
                    .artifacts
                    .latest(&artifact, Some(node))
                    .is_some_and(|held| since.is_none_or(|started| held.seq >= started));
                let why = unread.iter().find(|(of, _)| *of == artifact);
                let stands = match (accepted, why) {
                    (false, _) => DocumentStands::NotYet,
                    (true, None) => DocumentStands::Accepted,
                    (true, Some((_, why))) => DocumentStands::Unread { why: why.clone() },
                };
                (spec.to_string(), stands)
            })
            .collect();
        HandsOver { documents }
    }

    /// Its heading, then a row a document: its mark, its name, and where it stands, a column
    /// for every name; an unread one's why stands there, each of its lines wrapped at `look`'s
    /// width under that column. Nothing for a node that declares no artifact.
    pub fn blocks(&self, look: &Look) -> Vec<Block> {
        let Some(widest) = self
            .documents
            .iter()
            .map(|(name, _)| cell_width(name))
            .max()
        else {
            return Vec::new();
        };
        let column = widest + APART;
        let mut rows = Vec::new();
        for (name, stands) in &self.documents {
            let mark = look.glyphs.mark(stands.mark()).to_string();
            let under = " ".repeat(cell_width(&mark) + 1 + column);
            let room = look.width.cells().saturating_sub(cell_width(&under));
            let said = stands.said();
            let mut parts = said
                .lines()
                .flat_map(|line| wrap_unbroken(line.trim(), room));
            rows.push(
                Line::new()
                    .push(Tone::of(stands.mark()), mark)
                    .plain(format!(" {name:<column$}"))
                    .plain(parts.next().unwrap_or_default()),
            );
            rows.extend(parts.map(|part| Line::new().plain(format!("{under}{part}"))));
        }
        vec![Block::Heading(HEADING.to_string()), Block::Rows(rows)]
    }
}

impl DocumentStands {
    /// The mark a document's row opens with: what is accepted, what cannot be read, and
    /// what is not there yet.
    fn mark(&self) -> Mark {
        match self {
            DocumentStands::NotYet => Mark::Absent,
            DocumentStands::Accepted => Mark::Done,
            DocumentStands::Unread { .. } => Mark::Failed,
        }
    }

    /// What a document's row says of where it stands: an unread one, the lines of why it
    /// cannot be read.
    fn said(&self) -> &str {
        match self {
            DocumentStands::NotYet => "not yet",
            DocumentStands::Accepted => "the engine read it and accepted it",
            DocumentStands::Unread { why } => why,
        }
    }
}
