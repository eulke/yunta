//! The first request: what the application asks a person in a repository with no run yet,
//! and the workflows that could build it. The question is worded here once: the field's
//! first row asks the same, as a row of its list says it ([`FirstRequest::QUESTION`]), and a
//! new run asks it again over the request it goes on with, in the same box
//! ([`FirstRequest::boxed`]).
//! Each workflow stands on a row of its own, its name muted in a column of [`NAME_COLUMN`]
//! cells and what it does after it, so the workflows read down one edge and what they do down
//! another; a name too wide for the column keeps [`APART`] cells before what it does.
//! Its keys are the screen's (`screen::first_request_legend`); what the catalog prints
//! beyond a workflow's name and what it does, its inputs, its command and why a file does
//! not read, stays on the printed catalog.

use surco::components::TypedAnswer;
use surco::doc::{Block, Doc};
use surco::line::{Line, Tone};
use surco::look::Look;

use super::catalog::NameColumn;
use super::{Catalog, Page};
use crate::prompt::typed_answer;

/// What the box shows faint while nothing is typed in it: a request as a person writes one.
const PLACEHOLDER: &str = "add support for installing a pack globally";

/// The cells a workflow's name takes on its row before what it does: a pack's workflow by its
/// pack's name, `yunta/fragua`, and the cells that part it from what it does, as the design
/// sets the first request's list.
const NAME_COLUMN: usize = 16;

/// The fewest cells between a workflow's name and what it does, where the name passes its
/// column: two, so the gap reads as a parting, where one would read as a space between words.
const APART: usize = 2;

/// The first request, as a screen asks it: the question over the box the words are typed in,
/// the box showing what a request reads like while it is empty, then each workflow the
/// catalog offers that reads, with what it does, a row each, in the catalog's order, once the
/// catalog is read. Not `NewRun`, the screen a request goes on to.
pub struct FirstRequest<'a> {
    /// The words typed so far, before the cursor.
    pub before: &'a str,
    /// The words typed so far, after the cursor.
    pub after: &'a str,
    /// The workflows the repository can run, as its catalog read them; none until it is read.
    pub catalog: Option<&'a Catalog>,
}

impl FirstRequest<'_> {
    /// What the first request asks.
    pub const QUESTION: &'static str = "What do you want to build?";

    /// The question as it stands over the box: strong.
    pub fn asked() -> Line {
        Line::new().push(Tone::Strong, Self::QUESTION)
    }

    /// The question, and the box the words typed `before` and `after` the cursor stand in,
    /// its placeholder standing while it is empty.
    pub fn boxed(before: &str, after: &str) -> TypedAnswer {
        let asked = typed_answer(&Self::asked(), before.into(), after.into());
        TypedAnswer {
            placeholder: Some(PLACEHOLDER.to_string()),
            ..asked
        }
    }

    /// Each workflow the catalog offers that reads, a row each: its name muted in its column,
    /// and what it does after it, where it says. None before the catalog is read.
    fn workflows(&self) -> Option<Block> {
        let catalog = self.catalog?;
        let column = NameColumn {
            cells: NAME_COLUMN,
            apart: APART,
        };
        let rows = catalog.offered.iter().filter_map(|offered| {
            let readable = offered.workflow.as_ref().ok()?;
            let name = offered.name.as_str();
            let named = Line::new().push(Tone::Muted, name);
            Some(column.row(named, name, readable.does()))
        });
        Some(Block::Rows(rows.collect()))
    }
}

impl Page for FirstRequest<'_> {
    fn document(&self, _look: &Look) -> Doc {
        Doc::new()
            .with(Self::boxed(self.before, self.after))
            .with_all(self.workflows())
    }
}

#[cfg(test)]
mod tests;
