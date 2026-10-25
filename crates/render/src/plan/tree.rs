//! What the tree a plan was registered against held of the shapes it
//! builds, as a card tells it: under each shape's code, what that code is
//! to the code that stands where it goes; in the card's words, the files
//! beyond the plan whose code names a shape the plan changes.

use surco::components::{Against, Code, Fields};
use surco::text::counted;
use yunta_core::events::{InTree, Outside, ShapesReadPayload};
use yunta_core::Shape;

/// `code`, the code `shape` writes, told against what `read` found of it in
/// the tree: all new where its file held nothing of it, compared word by
/// word with what its file held of it, and beside something of its name
/// where that was not read as code to compare. With no reading of the
/// shape, it is only its lines.
pub(super) fn told(code: Code, shape: &Shape, read: Option<&ShapesReadPayload>) -> Code {
    match in_tree(shape, read) {
        None => code,
        Some(InTree::New) => code.against(None),
        Some(InTree::There {
            code: Some(today), ..
        }) => code.against(Some(today)),
        Some(InTree::There { code: None, .. }) => code.uncompared(),
    }
}

/// `fields` with what the shapes a card draws imply beyond the plan, each
/// of `drawn` beside its code, in the card's order: for a shape the tree
/// holds and the plan changes, each file whose code names it and no task
/// changes, or how many files carry its name where they are too many to
/// follow. Only the first row is labelled; the rest go on under it. Every
/// row is read as written: each names a shape or is a path.
pub(super) fn implies(
    mut fields: Fields,
    drawn: &[(&Shape, &Code)],
    read: Option<&ShapesReadPayload>,
) -> Fields {
    let rows = drawn
        .iter()
        .flat_map(|(shape, code)| implied(shape, code, read));
    for (n, row) in rows.enumerate() {
        fields = fields.push_literal(if n == 0 { "implies" } else { "" }, row);
    }
    fields
}

/// What `shape`, drawn as `code`, implies: nothing where the tree held none
/// of it, where its code keeps what stands there word for word, or where no
/// file beyond the plan names it; else each such file under a row that
/// counts them, or one row saying how many carry its name.
fn implied(shape: &Shape, code: &Code, read: Option<&ShapesReadPayload>) -> Vec<String> {
    let Some(InTree::There { outside, .. }) = in_tree(shape, read) else {
        return Vec::new();
    };
    if matches!(code.against, Some(Against::Unchanged)) {
        return Vec::new();
    }
    match outside {
        Outside::Files { paths } if paths.is_empty() => Vec::new(),
        Outside::Files { paths } => std::iter::once(format!(
            "{} is named in {} no task changes:",
            shape.name,
            counted(paths.len(), "file")
        ))
        .chain(paths.iter().cloned())
        .collect(),
        Outside::Common { files } => vec![format!(
            "{} is named in {}, too many to follow",
            shape.name,
            counted(*files, "file")
        )],
    }
}

/// What the tree held of `shape`, when `read` read it.
fn in_tree<'a>(shape: &Shape, read: Option<&'a ShapesReadPayload>) -> Option<&'a InTree> {
    read.and_then(|read| read.shape(&shape.name))
        .map(|read| &read.in_tree)
}
