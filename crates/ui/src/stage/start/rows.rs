//! See [`super`]. Moving along a new run's rows: the row in focus typed on, moved along or
//! changed; the focus moved from row to row, the workflow's row standing once the catalog is
//! read; the workflow, its mode and the adapter chosen, among the adapters the catalog says a
//! run here can be pinned to; the catalog taken as the new run opens, what the rows hold kept by
//! name where it still reads; and the row admission's refusal names.

use yunta_core::inputs::InputsProblem;
use yunta_core::{AdapterId, InputName, ModeName, Workflow};
use yunta_render::page::{Catalog, ReadableWorkflow};
use yunta_render::refusal::Refused;

use super::{Row, StartStage};
use crate::prompt::Stroke;

impl StartStage {
    /// `stroke` on the row in focus: typed on a typed row, moving the pointer on the
    /// workflows, changing the word chosen on the mode and the adapter.
    pub(super) fn on_the_row(&mut self, stroke: Stroke) {
        match (&self.focus, stroke) {
            (Row::Request, stroke) => {
                self.request.apply(stroke);
            }
            (Row::Input(name), stroke) => {
                self.inputs.entry(name.clone()).or_default().apply(stroke);
            }
            (Row::Workflow, Stroke::Up) => self.workflow_moved(false),
            (Row::Workflow, Stroke::Down) => self.workflow_moved(true),
            (Row::Mode, Stroke::Left) => self.mode_moved(false),
            (Row::Mode, Stroke::Right) => self.mode_moved(true),
            (Row::Adapter, Stroke::Left) => self.adapter_moved(false),
            (Row::Adapter, Stroke::Right) => self.adapter_moved(true),
            (Row::Workflow | Row::Mode | Row::Adapter, _) => {}
        }
    }

    /// The rows the keys reach, in order: the request, the workflow once the catalog is read,
    /// each other input the chosen workflow declares, its mode where it declares modes, and the
    /// adapter.
    pub(super) fn rows(&self) -> Vec<Row> {
        let chosen = self.chosen().map(|(_, workflow)| workflow);
        let inputs = chosen.into_iter().flat_map(|workflow| {
            workflow
                .inputs
                .iter()
                .filter(|(_, spec)| !spec.is_request())
                .map(|(name, _)| Row::Input(name.clone()))
        });
        let mode = chosen.and_then(declared_modes).map(|_| Row::Mode);
        let workflow = self.catalog.as_ref().map(|_| Row::Workflow);
        [Row::Request]
            .into_iter()
            .chain(workflow)
            .chain(inputs)
            .chain(mode)
            .chain([Row::Adapter])
            .collect()
    }

    /// The focus moved to the next row, `forward`, or the one before, round at either end.
    pub(super) fn focus_moved(&mut self, forward: bool) {
        let rows = self.rows();
        let at = rows.iter().position(|row| *row == self.focus).unwrap_or(0);
        self.focus = rows[stepped(at, rows.len(), forward)].clone();
    }

    /// `catalog`, read as the new run opens, taken as the one it is of: the workflow the rows
    /// hold kept where it still reads there, and its mode where it still declares it; else the
    /// first by how often each ran here, as moving to another workflow chooses. The adapter is
    /// kept where a run here can still be pinned to it, else each runner's own is. What was
    /// typed for each input stays by its name, and the focus goes back to the request where
    /// the row it stood on is gone.
    pub(super) fn read(&mut self, catalog: Box<Catalog>) {
        let mode = self.mode.take();
        self.adapter = self
            .adapter
            .take()
            .filter(|adapter| catalog.pinnable.contains(adapter));
        let kept = self
            .workflow
            .take()
            .filter(|name| readable(&catalog, name).is_some());
        let first = workflows(&catalog)
            .first()
            .map(|(name, _)| name.to_string());
        self.catalog = Some(catalog);
        match kept {
            Some(name) => {
                self.choose(Some(name));
                let declared = self
                    .chosen()
                    .and_then(|(_, workflow)| declared_modes(workflow));
                let declares =
                    |mode: &ModeName| declared.as_ref().is_some_and(|modes| modes.contains(mode));
                if let Some(mode) = mode.filter(declares) {
                    self.mode = Some(mode);
                }
            }
            None => self.choose(first),
        }
        if !self.rows().contains(&self.focus) {
            self.focus = Row::Request;
        }
    }

    /// The row admission's `refused` names: that of the input it is about, the request's where
    /// that input is the one the request stands in, where the chosen workflow declares it; none
    /// for a refusal about no one input.
    pub(super) fn row_refused(&self, refused: &Refused) -> Option<Row> {
        let Refused::Inputs(problem) = refused else {
            return None;
        };
        let name = input_named(problem)?;
        let (_, workflow) = self.chosen()?;
        let spec = workflow.inputs.get(name)?;
        Some(match spec.is_request() {
            true => Row::Request,
            false => Row::Input(name.clone()),
        })
    }

    /// The pointer moved to the next workflow that reads, `forward`, or the one before, round
    /// at either end.
    fn workflow_moved(&mut self, forward: bool) {
        let Some(catalog) = &self.catalog else {
            return;
        };
        let names: Vec<String> = workflows(catalog)
            .into_iter()
            .map(|(name, _)| name.to_string())
            .collect();
        let at = names
            .iter()
            .position(|name| Some(name) == self.workflow.as_ref())
            .unwrap_or(0);
        let next = names.get(stepped(at, names.len(), forward)).cloned();
        self.choose(next);
    }

    /// The mode moved to the next the workflow declares, `forward`, or the one before, round at
    /// either end.
    fn mode_moved(&mut self, forward: bool) {
        let Some(modes) = self
            .chosen()
            .and_then(|(_, workflow)| declared_modes(workflow))
        else {
            return;
        };
        let at = modes
            .iter()
            .position(|mode| Some(mode) == self.mode.as_ref())
            .unwrap_or(0);
        self.mode = modes.get(stepped(at, modes.len(), forward)).cloned();
    }

    /// The adapter moved to the next of none and each a run here can be pinned to, `forward`,
    /// or the one before, round at either end.
    fn adapter_moved(&mut self, forward: bool) {
        let adapters = adapters(self.catalog.as_deref());
        let at = adapters
            .iter()
            .position(|adapter| *adapter == self.adapter)
            .unwrap_or(0);
        self.adapter = adapters[stepped(at, adapters.len(), forward)].clone();
    }

    /// `name` chosen as the workflow, its mode opened on its first, as `yunta run` starts it
    /// when none is named; none where nothing is.
    fn choose(&mut self, name: Option<String>) {
        self.workflow = name;
        self.mode = self
            .chosen()
            .and_then(|(_, workflow)| declared_modes(workflow))
            .and_then(|modes| modes.into_iter().next());
    }

    /// The workflow chosen, by the name the catalog offers it under, as it reads; none until
    /// the catalog is read.
    pub(super) fn chosen(&self) -> Option<(&str, &Workflow)> {
        let (name, readable) = self.chosen_readable()?;
        Some((name, &readable.workflow))
    }

    /// The workflow chosen, by the name the catalog offers it under, as it reads with its past
    /// runs; none until the catalog is read.
    pub(super) fn chosen_readable(&self) -> Option<(&str, &ReadableWorkflow)> {
        let name = self.workflow.as_deref()?;
        Some((name, readable(self.catalog.as_deref()?, name)?))
    }
}

/// The place `steps` one row on from `at` among `count`, `forward` or back, round at either end.
fn stepped(at: usize, count: usize, forward: bool) -> usize {
    let count = count.max(1);
    match forward {
        true => (at + 1) % count,
        false => (at + count - 1) % count,
    }
}

/// The workflows `catalog` offers that read, each by its name, in the order a screen lists
/// them ([`Catalog::listed`]): the order a new run lists them in.
pub(super) fn workflows(catalog: &Catalog) -> Vec<(&str, &ReadableWorkflow)> {
    catalog
        .listed()
        .into_iter()
        .filter_map(|offered| Some((offered.name.as_str(), offered.workflow.as_ref().ok()?)))
        .collect()
}

/// The workflow `catalog` offers as `name`, where it reads.
fn readable<'c>(catalog: &'c Catalog, name: &str) -> Option<&'c ReadableWorkflow> {
    catalog
        .offered
        .iter()
        .find(|offered| offered.name == name)
        .and_then(|offered| offered.workflow.as_ref().ok())
}

/// The modes `workflow` declares, in its order, the first first; none where it declares none.
pub(super) fn declared_modes(workflow: &Workflow) -> Option<Vec<ModeName>> {
    let modes = workflow.modes.as_ref().filter(|modes| !modes.is_empty())?;
    Some(modes.keys().cloned().collect())
}

/// None, which leaves each session to the adapter its runner names, as `yunta run` does when
/// none is named, then each adapter a run of `catalog` can be pinned to, in the order this
/// binary builds them: none but the first where the runners name none, or until the catalog is
/// read.
pub(super) fn adapters(catalog: Option<&Catalog>) -> Vec<Option<AdapterId>> {
    let pinnable = catalog.into_iter().flat_map(|catalog| &catalog.pinnable);
    std::iter::once(None)
        .chain(pinnable.cloned().map(Some))
        .collect()
}

/// The input `problem` is about: every problem but the names a workflow does not declare,
/// which are about several or none.
fn input_named(problem: &InputsProblem) -> Option<&InputName> {
    match problem {
        InputsProblem::Unknown { .. } => None,
        InputsProblem::Missing { name }
        | InputsProblem::InvalidNumber { name, .. }
        | InputsProblem::InvalidBoolean { name, .. }
        | InputsProblem::BelowMin { name, .. }
        | InputsProblem::AboveMax { name, .. }
        | InputsProblem::TooShort { name, .. }
        | InputsProblem::PatternMismatch { name, .. }
        | InputsProblem::NotInEnum { name, .. }
        | InputsProblem::PathNotFound { name, .. }
        | InputsProblem::DocumentUnreadable { name, .. }
        | InputsProblem::DocumentRefused { name, .. }
        | InputsProblem::InvalidPattern { name, .. } => Some(name),
    }
}
