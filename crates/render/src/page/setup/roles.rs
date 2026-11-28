//! See [`super`]. A role's row on a setup: its candidates in their columns, or, where it is
//! being changed in place, the adapters this machine answers for with one chosen and its
//! model typed in a box under them; a role no adapter answers for says so.

use surco::components::{SurveyRow, SurveyValue, TypedInPlace, WordChoice};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::width::cell_width;
use yunta_core::MachineAdapters;

use super::{Candidates, Pointed, ProposedCandidate, ProposedRole, Setup, FIRST_CANDIDATE_CELLS};
use crate::prompt::{MODEL, NONE_LEAVES_IT_TO_THE_ADAPTER};

/// The cells at least between a role's first candidate and the others where the first takes
/// its whole column: two, as a checklist's subjects stand apart from what they found.
const GAP: usize = 2;

impl Setup<'_> {
    /// The row of the role `proposed`: the adapter that would do its work first, with its model
    /// where one was typed, and where `candidates` says, the others muted in a column of their
    /// own after it; changed, the adapters this machine answers for with one chosen, its model
    /// typed under it. A role no adapter answers for says so, and names the ones this build
    /// knows.
    pub(super) fn role_row(
        &self,
        proposed: &ProposedRole<'_>,
        candidates: Candidates,
    ) -> SurveyRow {
        let (pointed, choosing) = match &self.pointed {
            Some(Pointed::Role(runner, choosing)) if *runner == proposed.runner => (true, choosing),
            _ => (false, &None),
        };
        let row = |mark: Mark, value: SurveyValue, typed: Option<TypedInPlace>| SurveyRow {
            mark,
            label: proposed.runner.to_string(),
            value,
            pointed,
            typed,
        };
        if let Some(choosing) = choosing {
            let choice = WordChoice {
                words: (choosing.adapters.iter())
                    .map(|adapter| adapter.to_string())
                    .collect(),
                chosen: choosing.chosen,
                focused: !choosing.on_the_model,
            };
            let model = &choosing.model;
            let typed = TypedInPlace {
                label: MODEL.to_string(),
                typed: crate::prompt::typed_row(model.before.clone(), model.after.clone()),
                cells: FIRST_CANDIDATE_CELLS,
                hint: Some(NONE_LEAVES_IT_TO_THE_ADAPTER.to_string()),
                focused: choosing.on_the_model,
            };
            return row(Mark::Done, SurveyValue::Chosen(choice), Some(typed));
        }
        let Some((first, rest)) = proposed.candidates.split_first() else {
            let known = MachineAdapters::probed(self.probed).known;
            let said = crate::declare::none_answered(&known);
            return row(Mark::Caution, SurveyValue::Found(said), None);
        };
        let first = first.said();
        let line = Line::new().plain(first.as_str());
        let line = match (candidates, rest.is_empty()) {
            (Candidates::Every, false) => line.then(others(&first, rest)),
            (Candidates::Every | Candidates::First, _) => line,
        };
        row(Mark::Done, SurveyValue::Composed(line), None)
    }
}

/// The candidates of a role after its `first`, muted, where the column the first takes ends:
/// `then` and each of `rest` in the order it is written.
fn others(first: &str, rest: &[ProposedCandidate<'_>]) -> Line {
    let taken = cell_width(first);
    let pad = match taken + GAP <= FIRST_CANDIDATE_CELLS {
        true => FIRST_CANDIDATE_CELLS - taken,
        false => GAP,
    };
    let others: Vec<String> = rest.iter().map(ProposedCandidate::said).collect();
    Line::new().plain(" ".repeat(pad)).push(
        Tone::Muted,
        format!("{} {}", Setup::THEN, others.join(", ")),
    )
}
