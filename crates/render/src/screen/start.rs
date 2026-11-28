//! See [`super`]. A run started, and the first one: their keys, a new run's by the row in
//! focus, each row's own key first. What the first request asks is its page's
//! (`page::FirstRequest`), and what a new run's rows say is theirs (`page::NewRun`); a new
//! run stands on the trail as `n` names it ([`super::NEW_RUN`]).

use surco::components::Legend;
use yunta_core::{InputName, InputSpec};

use super::{BACK, FIELD, NEW_RUN};
use crate::page::NewRun;
use crate::prompt::{ENTER, ESC};

/// The keys of the first request, where a line is typed: what typing does, then the keys
/// that act and do not type, going on to a new run with what is typed, and `esc`, which goes
/// to the field, whatever it holds. No far pair, since `/` and `?` type there too.
pub fn first_request_legend() -> Legend {
    Legend::new()
        .typing("what you want to build")
        .key(ENTER, NEW_RUN)
        .key(ESC, FIELD)
}

/// A row of a new run a person sets, as its keys act on it: the request and each other input
/// typed, the workflow moved along, the mode and the adapter chosen among their words.
pub enum NewRunRow<'a> {
    /// The request, which the workflow reads from the input that takes it.
    Request,
    /// An input other than the request, named `name`, as the workflow declares it.
    Input {
        name: &'a InputName,
        spec: &'a InputSpec,
    },
    Workflow,
    Mode,
    Adapter,
}

/// The keys of a new run, where every printable key types on a typed row: the key of the row
/// in focus first (typing it, moving along the workflows, changing the mode or the adapter),
/// then going to the next row, starting the run, and going back. No far pair, since a line is
/// typed there.
pub fn new_run_legend(row: NewRunRow) -> Legend {
    let first = match row {
        NewRunRow::Request => Legend::new().typing("the request"),
        NewRunRow::Input { name, spec } => Legend::new().typing(typed_on(name, spec)),
        NewRunRow::Workflow => Legend::new().arrows(NewRun::WORKFLOW),
        NewRunRow::Mode => Legend::new().left_right(NewRun::MODE),
        NewRunRow::Adapter => Legend::new().left_right(NewRun::ADAPTER),
    };
    first
        .key("tab", "next row")
        .key(ENTER, "start")
        .key(ESC, BACK)
}

/// What a person types on input `name`'s row, as its legend says it: the input by its name,
/// then the kind of value it takes where that is more than words: `the tasks document`.
fn typed_on(name: &InputName, spec: &InputSpec) -> String {
    match spec {
        InputSpec::String { .. } => format!("the {name}"),
        _ => format!("the {name} {}", crate::inputs::type_label(spec)),
    }
}
