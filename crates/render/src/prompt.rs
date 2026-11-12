//! What a prompt says, apart from where it stands: what Escape does where it opens,
//! the key that leaves it and the mark an answer stands after, the keys every list
//! answers to, what an answer pasted over several lines is said as, and what a
//! command asked to ask says when no terminal is there to ask on.
//!
//! The prompts' models in `yunta-ui` keep where each stands and what a stroke does
//! there; every sentence they show a person is said here, so a terminal, a page and a
//! window say a prompt alike.

use surco::components::{ChoiceList, Choices, Legend, TypedAnswer, TypedRow};
use surco::line::{Line, Tone};
use yunta_core::{Responder, RunnerName, WorkflowShape};

pub mod card;
pub mod desk;
pub mod round;

/// What Escape ends a prompt with, which is not the same thing everywhere: a prompt a
/// run stopped on parks that run, and a prompt in a command with no run behind it
/// leaves the value the command worked out on its own. One enum, because the gesture
/// is one gesture and only its consequence differs, and it is said wherever a prompt
/// opens, so a person never has to guess which of the two they are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escape {
    /// The run this prompt belongs to parks with its state intact.
    Parks,
    /// The run, parked already, keeps waiting with nothing recorded.
    LeavesWaiting,
    /// The value the command detected for itself stands.
    KeepsDefault,
    /// A choice with no detected value: Escape takes the first, the plainest of them.
    TakesTheFirst,
    /// A question of several a person is taken through: Escape goes back to where they
    /// came from.
    GoesBack,
}

impl Escape {
    /// The phrase, as every prompt says it: the key, then what it does.
    pub fn said(self) -> String {
        format!("{ESC} {}", self.does())
    }

    /// What Escape does, as a legend says it beside the key.
    pub fn does(self) -> &'static str {
        match self {
            Escape::Parks => "leaves it for later",
            Escape::LeavesWaiting => "leaves the run waiting",
            Escape::KeepsDefault => "keeps the detected default",
            Escape::TakesTheFirst => "takes the first",
            Escape::GoesBack => "goes back",
        }
    }
}

/// The key a person leaves a prompt with, as a legend spells it.
pub const ESC: &str = "esc";

/// What marks a line an answer is given on, typed or echoed back, so a person reads a
/// round as answers under the things that asked for them.
pub const ANSWER: &str = "> ";

/// Said once, when the answer being recorded came from text that spanned more than one
/// line: a question takes one line, and the difference belongs to whoever pasted it
/// rather than to a silent truncation.
pub const FOLDED: &str = "the text pasted here spanned more than one line, \
                          and this answer is those lines joined into one";

/// The keys any list answers to, whatever holds it.
pub fn list_legend() -> Legend {
    Legend::new().arrows("move").typing("to filter")
}

/// What `--interactive` says when there is no terminal to ask on, before the
/// `fallback` the command takes instead.
pub fn no_terminal(fallback: &str) -> String {
    format!("--interactive given but there is no terminal to ask on; {fallback}")
}

/// What `init -i` takes when no terminal is there to ask on.
pub const USING_DETECTED_DEFAULTS: &str = "using detected defaults";

/// What `new -i` takes when no terminal is there to ask on: the shape named `shape`.
pub fn defaulting_to(shape: &str) -> String {
    format!("defaulting to `{shape}`")
}

/// A line asked, and the row its answer is typed on: the words before the cursor and
/// after it, whole, after the mark an answer stands after.
pub fn typed_answer(asked: &Line, before: String, after: String) -> TypedAnswer {
    TypedAnswer {
        asked: asked.clone(),
        typed: TypedRow {
            prompt: ANSWER.to_string(),
            before,
            after,
        },
    }
}

/// A list a person picks from: what the filter leaves, what was typed, and the keys it
/// answers to with what Escape does where it opened.
pub fn choice_list(choices: Option<Choices>, filter: Option<&str>, escape: Escape) -> ChoiceList {
    ChoiceList {
        choices,
        filter: filter.map(str::to_string),
        legend: list_legend()
            .key("enter", "chooses")
            .key(ESC, escape.does()),
    }
}

/// What `init -i` asks a project's name with.
pub const PROJECT_NAME: &str = "project name";

/// What `init -i` asks a project's base branch with.
pub const BASE_BRANCH: &str = "base branch";

/// A setting `asked` for with the value `kept` that stands where none is given.
pub fn keeping(asked: &str, kept: &str) -> Line {
    Line::new()
        .push(Tone::Strong, asked)
        .push(Tone::Muted, format!(" (enter keeps `{kept}`)"))
}

/// What `init -i` says above the adapters it offers `runner`: the ones this machine
/// answers for.
pub fn runs_on(runner: &RunnerName) -> String {
    format!("the adapter `{runner}` runs on")
}

/// What `init -i` asks `runner`'s model with: it has no default, since the model is
/// the project's to choose, and a line left empty leaves the runner to declare later.
pub fn model_of(runner: &RunnerName) -> Line {
    Line::new()
        .push(Tone::Strong, format!("the model `{runner}` runs"))
        .push(Tone::Muted, " (enter with none leaves it to declare later)")
}

/// What `new -i` says above the shapes it offers.
pub const CHOOSE_A_SHAPE: &str = "choose a shape";

/// What a workflow of `shape` does, beside its name on a menu.
pub fn shape_does(shape: WorkflowShape) -> &'static str {
    match shape {
        WorkflowShape::OneNode => "one command whose exit code is the check",
        WorkflowShape::LintFix => "a check, and an agent that fixes what it finds",
        WorkflowShape::Tasks => "a plan of tasks, and an agent that works through them",
    }
}

/// The line that says whom an answer is recorded as, `responder`: nothing where a person
/// types authenticates them, and saying so as it is recorded keeps an unsigned answer
/// from reading like a signed one.
pub fn attributed(responder: &Responder) -> String {
    format!("attributed to `{responder}`: unsigned, nothing here authenticates who typed")
}
