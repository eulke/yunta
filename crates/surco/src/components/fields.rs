//! Labelled facts, one to a row, the label in a column of its own.

use crate::line::Tone;

/// Labelled rows. A fact with nothing to say is not a row: a label over
/// an empty value tells a reader to look for something that is not
/// there.
#[derive(Default)]
pub struct Fields {
    pub(crate) rows: Vec<(&'static str, String, Tone)>,
}

impl Fields {
    pub fn new() -> Self {
        Fields::default()
    }

    /// These fields with `value` under `label`, when it says anything.
    pub fn push_if(mut self, label: &'static str, value: impl Into<String>) -> Self {
        let value = value.into();
        if !value.trim().is_empty() {
            self.rows.push((label, value, Tone::Plain));
        }
        self
    }

    /// `command` beside `label`: a row a reader copies.
    pub fn push_command(mut self, label: &'static str, command: impl Into<String>) -> Self {
        let command = command.into();
        if !command.trim().is_empty() {
            self.rows.push((label, command, Tone::Command));
        }
        self
    }
}

impl Fields {
    /// Each row: its label, and what it says.
    pub fn rows(&self) -> impl Iterator<Item = (&str, &str, Tone)> {
        self.rows
            .iter()
            .map(|(label, value, tone)| (*label, value.as_str(), *tone))
    }
}
