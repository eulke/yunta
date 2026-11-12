//! The tests a plan's tasks are held to, when they are shown without the
//! plan: task by task, what each test proves and the command that runs
//! it, then the file it lives in.

use surco::components::{Code, Fields, Section};
use surco::doc::{Block, Depth, Doc};
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::text::counted;
use yunta_core::step::NextStep;
use yunta_core::{NodeId, RunId, SpecFile};

use super::plan::Form;

/// The lines of a file of tests a spec's page shows before the rest is a
/// command away: the length of a test that checks one behavior, so a page
/// shows what a task's tests do without one file pushing every other task
/// off the screen.
pub const CODE_SHOWN: usize = 60;

/// The tests a plan's tasks are held to, as the page a person reads when
/// they are shown without the plan: `file`, written by `producer` when the
/// run's log says — which `of` names — for the run `run`, in `form`.
#[derive(Debug, Clone, Copy)]
pub struct Spec<'a> {
    pub file: &'a SpecFile,
    pub of: &'a str,
    pub producer: Option<&'a NodeId>,
    pub run: &'a RunId,
    pub form: Form,
}

impl super::Page for Spec<'_> {
    fn document(&self, _look: &Look) -> Doc {
        document(self.file, (self.of, self.producer), self.run, self.form)
    }
}

/// The tests of `file`, written by `node` when the run's log says — which
/// `of` names — for the run `run`, the one each command a reader copies takes.
fn document(file: &SpecFile, (of, node): (&str, Option<&NodeId>), run: &RunId, form: Form) -> Doc {
    let tests = test_count(file);
    let mut doc = Doc::new().with(Block::Title(surco::components::Title::named(
        Line::new()
            .push(Tone::Strong, format!("spec{of}"))
            .plain(format!(
                ": {} for {}",
                counted(tests, "test"),
                counted(file.specs.len(), "task")
            )),
    )));
    for spec in &file.specs {
        let mut fields = Fields::new();
        for test in &spec.tests {
            fields = fields
                .push_if("proves", test.proves.as_str())
                .push_command("", test.cmd.as_str());
        }
        let mut blocks = vec![fields.into()];
        for test_file in &spec.files {
            let path = test_file.path.as_str();
            let content = &test_file.content;
            let whole_at = node.map(|node| status_command(run, node));
            blocks.push(
                match (form, whole_at) {
                    (Form::Review, Some(whole_at)) => {
                        Code::reviewed(path, None, content, CODE_SHOWN, whole_at)
                    }
                    _ => Code::whole(path, None, content),
                }
                .into(),
            );
        }
        doc = doc.with(Section {
            lead: None,
            title: Line::new().push(Tone::Strong, spec.task.as_str()),
            blocks,
        });
    }
    doc
}

/// How many tests `file` holds, across every task it gives tests to.
pub(crate) fn test_count(file: &SpecFile) -> usize {
    file.specs.iter().map(|spec| spec.tests.len()).sum()
}

/// The command that shows a spec's tests, whole, for the run `run`: the
/// status of `node`, which wrote it.
pub(crate) fn status_command(run: &RunId, node: &NodeId) -> String {
    crate::command::line(&NextStep::Status {
        run: run.clone(),
        node: Some(node.clone()),
        depth: Depth::Glance,
    })
}
