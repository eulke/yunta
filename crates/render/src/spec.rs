//! The tests a plan's tasks are held to, as the person approving them
//! reads them on a terminal: task by task, what each test proves, then
//! the files the tests live in, whole — what is approved is what is read.

use yunta_core::text::counted;
use yunta_core::SpecFile;

use crate::markdown::{hanging, markdown};
use crate::INDENT;

/// Where a task's tests sit: one step under its id.
const BODY: &str = "    ";

pub(super) fn spec(file: &SpecFile, of: &str, width: usize) -> Vec<String> {
    let tests: usize = file.specs.iter().map(|spec| spec.tests.len()).sum();
    let mut lines = vec![format!(
        "the spec{of} — {} for {}",
        counted(tests, "test"),
        counted(file.specs.len(), "task")
    )];
    for spec in &file.specs {
        lines.push(String::new());
        lines.push(format!("{INDENT}{}", spec.task));
        for test in &spec.tests {
            lines.extend(hanging(BODY, "proves ", &test.proves, width));
        }
        for test_file in &spec.files {
            lines.push(String::new());
            lines.extend(hanging(BODY, "", &test_file.path, width));
            let content = format!("```\n{}\n```", test_file.content.trim_end());
            lines.extend(markdown(&content, BODY, width));
        }
    }
    lines
}
