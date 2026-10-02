//! The tests a plan's tasks are held to, when they are shown without the
//! plan: task by task, what each test proves and the command that runs
//! it, then the file it lives in.

use yunta_core::text::counted;
use yunta_core::SpecFile;

use crate::blocks::{Code, Fields, Section};
use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};
use crate::plan::{Form, CODE_SHOWN};

pub fn document(file: &SpecFile, of: &str, run: &str, form: Form) -> Doc<'static> {
    let tests: usize = file.specs.iter().map(|spec| spec.tests.len()).sum();
    let mut doc = Doc::new().with(Block::Title(
        Line::new()
            .push(Tone::Strong, format!("spec{of}"))
            .plain(format!(
                ": {} for {}",
                counted(tests, "test"),
                counted(file.specs.len(), "task")
            )),
    ));
    for spec in &file.specs {
        let mut fields = Fields::new();
        for test in &spec.tests {
            fields = fields
                .push_if("proves", test.proves.as_str())
                .push_command("", format!("$ {}", test.cmd));
        }
        let mut blocks = vec![fields.into()];
        for test_file in &spec.files {
            let code = Code::whole(test_file.path.as_str(), None, &test_file.content);
            blocks.push(
                match form {
                    Form::Review => code.cut(CODE_SHOWN, format!("yunta status {run} --node spec")),
                    Form::Whole => code,
                }
                .into(),
            );
        }
        doc = doc.with(Section {
            mark: None,
            title: Line::new().push(Tone::Strong, spec.task.as_str()),
            blocks,
        });
    }
    doc
}
