//! The tests a plan's tasks are held to, when they are shown without the
//! plan: task by task, what each test proves and the command that runs
//! it, then the file it lives in.

use yunta_core::step::NextStep;
use yunta_core::text::counted;
use yunta_core::{NodeId, RunId, SpecFile};

use crate::blocks::{Code, Fields, Section};
use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};
use crate::plan::{Form, CODE_SHOWN};

/// The tests of `file`, for the run `run`; each command a reader copies
/// is written by `spell`.
pub fn document(
    file: &SpecFile,
    of: &str,
    run: &RunId,
    form: Form,
    spell: &dyn Fn(&NextStep) -> String,
) -> Doc<'static> {
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
            let path = test_file.path.as_str();
            let content = &test_file.content;
            blocks.push(
                match form {
                    Form::Review => {
                        Code::reviewed(path, None, content, CODE_SHOWN, status_command(run, spell))
                    }
                    Form::Whole => Code::whole(path, None, content),
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

/// The command that shows the spec's text, whole, for the run `run`: the
/// status of the node that holds it, which `spell` writes.
pub(crate) fn status_command(run: &RunId, spell: &dyn Fn(&NextStep) -> String) -> String {
    spell(&NextStep::Status {
        run: run.clone(),
        node: Some(NodeId::from_static("spec")),
    })
}
