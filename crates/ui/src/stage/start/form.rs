//! See [`super`]. A new run's rows as one Surco form, each in render's words (`page::NewRun`):
//! the request in the first request's box, under its question and over its hint; the
//! workflows that read, by how often each ran here, the pointer on the one chosen; each other
//! input the chosen workflow declares, labelled over its box and described under it; its modes
//! where it declares any; and the adapters; then, at the form's foot, what the chosen workflow
//! runs in the chosen mode, which moves with those two alone. A row's heading is strong where
//! the keys act on it, and what admission refused of a typed row stands under it. The
//! workflows' row stands once the catalog is read.

use surco::components::{ChoiceRow, Choices, Form, FormRow, FormValue, TypedRow, WordChoice};
use surco::glyphs::Glyphs;
use yunta_core::Workflow;
use yunta_render::page::{FirstRequest, NewRun};

use super::rows::{adapters, declared_modes, workflows};
use super::{Row, StartStage};

impl StartStage {
    /// The rows as one form, the keys on the row in focus, what runs under them in `glyphs`:
    /// what the chosen workflow runs in the chosen mode.
    pub(super) fn form(&self, glyphs: Glyphs) -> Form {
        let chosen = self.chosen().map(|(_, workflow)| workflow);
        let mode =
            chosen.map(|workflow| self.mode.clone().unwrap_or_else(|| workflow.first_mode()));
        let (before, after) = self.request.split();
        let new_run = chosen
            .zip(mode.as_ref())
            .map(|(workflow, mode)| NewRun { workflow, mode });
        let request = match &new_run {
            Some(new_run) => new_run.request(&before, &after),
            None => FirstRequest::boxed(&before, &after),
        };
        let workflows = self.workflows_listed().map(|listed| FormRow {
            label: Some(NewRun::heading(
                NewRun::WORKFLOW,
                self.focus == Row::Workflow,
            )),
            value: FormValue::Listed(listed),
            description: None,
            refusal: None,
        });
        let declared = chosen.into_iter().flat_map(|workflow| {
            self.input_rows(workflow)
                .into_iter()
                .chain(self.mode_row(workflow))
        });
        let typed = typed_row(request.typed, request.placeholder, None);
        let rows = [FormRow {
            refusal: self.refused_on(&Row::Request),
            ..typed
        }]
        .into_iter()
        .chain(workflows)
        .chain(declared)
        .chain([self.adapter_row()])
        .collect();
        let focus = self.rows().iter().position(|row| *row == self.focus);
        Form {
            asked: Some(request.asked),
            hint: request.hint,
            rows,
            focus,
            foot: new_run.map(|new_run| new_run.what_runs(glyphs)),
        }
    }

    /// What admission refused of what `row` holds, where it stands in focus with a refusal under
    /// it.
    fn refused_on(&self, row: &Row) -> Option<String> {
        self.refusal.clone().filter(|_| self.focus == *row)
    }

    /// The workflows the catalog offers that read, a row each with what it does, by how often
    /// each ran here, the pointer on the one chosen; none until the catalog is read.
    fn workflows_listed(&self) -> Option<Choices> {
        let rows: Vec<ChoiceRow> = workflows(self.catalog.as_deref()?)
            .into_iter()
            .map(|(name, readable)| ChoiceRow {
                name: name.to_string(),
                said: readable.does(),
                cost: None,
                key: None,
            })
            .collect();
        let at = rows
            .iter()
            .position(|row| Some(&row.name) == self.workflow.as_ref());
        Some(Choices {
            rows,
            at: at.unwrap_or(0),
        })
    }

    /// A row for each input other than the request `workflow` declares, as typed so far, what
    /// admission refused of it under it.
    fn input_rows(&self, workflow: &Workflow) -> Vec<FormRow> {
        let inputs = workflow
            .inputs
            .iter()
            .filter(|(_, spec)| !spec.is_request());
        inputs
            .map(|(name, spec)| {
                let (before, after) = self.inputs.get(name).cloned().unwrap_or_default().split();
                let row = Row::Input(name.clone());
                let asked = NewRun::input(name, spec, (&before, &after), self.focus == row);
                FormRow {
                    label: Some(asked.asked),
                    refusal: self.refused_on(&row),
                    ..typed_row(asked.typed, asked.placeholder, asked.hint)
                }
            })
            .collect()
    }

    /// The modes `workflow` declares, the one chosen marked; none where it declares none.
    fn mode_row(&self, workflow: &Workflow) -> Option<FormRow> {
        let modes = declared_modes(workflow)?;
        let chosen = modes
            .iter()
            .position(|mode| Some(mode) == self.mode.as_ref());
        let words = modes.iter().map(ToString::to_string).collect();
        Some(words_row(
            NewRun::MODE,
            words,
            chosen,
            self.focus == Row::Mode,
        ))
    }

    /// No adapter, said as the runners say, then each a run here can be pinned to, the one
    /// chosen marked.
    fn adapter_row(&self) -> FormRow {
        let adapters = adapters(self.catalog.as_deref());
        let chosen = adapters.iter().position(|adapter| *adapter == self.adapter);
        let words = adapters
            .iter()
            .map(|adapter| match adapter {
                Some(adapter) => adapter.as_str().to_string(),
                None => NewRun::AS_THE_RUNNERS_SAY.to_string(),
            })
            .collect();
        words_row(NewRun::ADAPTER, words, chosen, self.focus == Row::Adapter)
    }
}

/// A typed row of a form, unlabelled: `typed` in its box, `placeholder` faint in it while it is
/// empty, and `description` under it.
fn typed_row(typed: TypedRow, placeholder: Option<String>, description: Option<String>) -> FormRow {
    FormRow {
        label: None,
        value: FormValue::Typed { typed, placeholder },
        description,
        refusal: None,
    }
}

/// A row of `words` under `heading`, the one at `chosen` chosen, the first where none is; the
/// keys on it where it is `focused`.
fn words_row(heading: &str, words: Vec<String>, chosen: Option<usize>, focused: bool) -> FormRow {
    FormRow {
        label: Some(NewRun::heading(heading, focused)),
        value: FormValue::Words(WordChoice {
            words,
            chosen: chosen.unwrap_or(0),
            focused,
        }),
        description: None,
        refusal: None,
    }
}
