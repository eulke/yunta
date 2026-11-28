//! What a prompt says, apart from where it stands: what each key does there and what
//! Escape does where it opens, the key that leaves it and the mark an answer stands
//! after, the keys every list answers to, what an answer pasted over several lines is
//! said as, and what a command asked to ask says when no terminal is there to ask on.
//!
//! The prompts' models in `yunta-ui` keep where each stands and what a stroke does
//! there; every sentence they show a person is said here, so a terminal, a page and a
//! window say a prompt alike. A key's act is said once, as an order ([`PromptAct`]): a
//! prompt at a terminal and a run's screen may take other keys for one act, never other
//! words, and each legend a screen shares with a prompt is built once, here.

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
    /// A row changed in place stands as a person changed it before it was opened.
    KeepsItAsItWas,
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

    /// What Escape does, as a legend says it beside the key: an order, in the words a
    /// screen says the same act in.
    pub fn does(self) -> &'static str {
        match self {
            Escape::Parks => PromptAct::LeaveItForLater.said(),
            Escape::LeavesWaiting => PromptAct::LeaveItWaiting.said(),
            Escape::KeepsDefault => "keep the detected default",
            Escape::KeepsItAsItWas => "keep it as it was",
            Escape::TakesTheFirst => "take the first",
            Escape::GoesBack => PromptAct::GoBack.said(),
        }
    }
}

/// What a key does on a prompt, said once as an order, so a prompt at a terminal and a
/// run's screen say one act in the same words whatever key does it there; a confirmation's
/// acts among them, each the act it asks about and keeping things as they are; and the acts
/// a screen's legend names that a prompt shares or stops: work under way stopped, the
/// palette's row taken, leaving. Not what Escape comes to where a prompt opens ([`Escape`]),
/// which says some of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAct {
    /// The pointer moved to another of what is offered.
    Choose,
    /// What is pointed at taken, or what is typed given, as the answer.
    Answer,
    /// What a number named taken.
    Confirm,
    /// The words an option asks for sent with it.
    SendIt,
    /// The note typed recorded with the option.
    RecordIt,
    /// A line opened for a note on the option.
    AddANote,
    /// The plan a decision is on opened to be read.
    ReadThePlan,
    /// Back to the choice before.
    GoBack,
    /// Away from a decision, which stays open.
    LeaveItWaiting,
    /// Away from a round of questions, nothing of it recorded.
    LeaveItForLater,
    /// The run a confirmation names closed for good.
    CloseIt,
    /// The runs a confirmation names, several, closed for good.
    CloseThem,
    /// The run a confirmation names stopped, with everything under it; or a collection under
    /// way stopped.
    StopIt,
    /// The cases under way stopped.
    StopThem,
    /// The sessions doctor opens, under way, stopped.
    StopTheSessions,
    /// The pack a confirmation names removed from the project.
    RemoveIt,
    /// The pack a confirmation names installed.
    InstallIt,
    /// The setup written without the pack its confirmation names, which is let go.
    WriteWithoutIt,
    /// A session opened on each agent binding.
    OpenThem,
    /// What a dry run found collected.
    CollectIt,
    /// Away from a confirmation, what it would change kept as it is.
    KeepIt,
    /// Away from a confirmation of an act on several runs, each kept as it is.
    KeepThem,
    /// A row opened to be changed in place.
    ChangeIt,
    /// What is chosen or typed in a row changed in place taken into it.
    TakeIt,
    /// The act a palette's row offers done.
    DoIt,
    /// The application left.
    Quit,
}

impl PromptAct {
    /// The act, as a legend says it beside its key.
    pub fn said(self) -> &'static str {
        match self {
            PromptAct::Choose => "choose",
            PromptAct::Answer => "answer",
            PromptAct::Confirm => "confirm",
            PromptAct::SendIt => "send it",
            PromptAct::RecordIt => "record it",
            PromptAct::AddANote => "add a note",
            PromptAct::ReadThePlan => "read the plan",
            PromptAct::GoBack => "go back",
            PromptAct::LeaveItWaiting => "leave it waiting",
            PromptAct::LeaveItForLater => "leave it for later",
            PromptAct::CloseIt => "close it",
            PromptAct::CloseThem => "close them",
            PromptAct::StopIt => "stop it",
            PromptAct::StopThem => "stop them",
            PromptAct::StopTheSessions => "stop the sessions",
            PromptAct::RemoveIt => "remove it",
            PromptAct::InstallIt => "install it",
            PromptAct::WriteWithoutIt => "write without it",
            PromptAct::OpenThem => "open them",
            PromptAct::CollectIt => "collect it",
            PromptAct::KeepIt => "keep it",
            PromptAct::KeepThem => "keep them",
            PromptAct::ChangeIt => "change it",
            PromptAct::TakeIt => "take it",
            PromptAct::DoIt => "do it",
            PromptAct::Quit => "quit",
        }
    }
}

/// The key a person leaves a prompt with, as a legend spells it.
pub const ESC: &str = "esc";

/// The key a person takes what is pointed at with, as a palette's key column spells it
/// where an act is bound to it.
pub const ENTER: &str = "enter";

/// What marks a line an answer is given on, typed or echoed back, so a person reads a
/// round as answers under the things that asked for them.
pub const ANSWER: &str = "> ";

/// Said once, when the answer being recorded came from text that spanned more than one
/// line: a question takes one line, and the difference belongs to whoever pasted it
/// rather than to a silent truncation.
pub const FOLDED: &str = "the text pasted here spanned more than one line, \
                          and this answer is those lines joined into one";

/// The digits that pick an option where a prompt numbers its options: `1` to `9`, since a
/// second digit would have nothing to say the first has ended.
const DIGITS: usize = 9;

/// The digits that pick each of `options`, as a legend names them in one entry: `1 2 3`.
pub(crate) fn numbered(options: usize) -> String {
    let digits: Vec<String> = (1..=options.min(DIGITS)).map(|n| n.to_string()).collect();
    digits.join(" ")
}

/// The keys any list answers to, whatever holds it: the arrows choose, and what is typed
/// narrows it on a row of its own, which says so.
pub fn list_legend() -> Legend {
    Legend::new().arrows(PromptAct::Choose.said())
}

/// The keys of a number chosen at a plan's desk, waiting to be confirmed: `enter` confirms
/// it, and `esc` goes back. Not a confirmation over a screen, whose keys name its act
/// (`screen::Confirming::legend`).
pub fn confirming_legend() -> Legend {
    Legend::new()
        .key(ENTER, PromptAct::Confirm.said())
        .key(ESC, PromptAct::GoBack.said())
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
/// after it, whole, after the mark an answer stands after. What it asks says it all, so
/// the box holds no placeholder and nothing stands under it.
pub fn typed_answer(asked: &Line, before: String, after: String) -> TypedAnswer {
    TypedAnswer {
        asked: asked.clone(),
        typed: typed_row(before, after),
        placeholder: None,
        hint: None,
    }
}

/// The row words are typed on, before the cursor and after it, opened as an answer is
/// where it is written on one line: an answer's, a card's words, a setup's row changed in
/// place.
pub(crate) fn typed_row(before: String, after: String) -> TypedRow {
    TypedRow {
        prompt: ANSWER.to_string(),
        before,
        after,
    }
}

/// A list a person picks from: what the filter leaves, what was typed, and the keys it
/// answers to with what Escape does where it opened.
pub fn choice_list(choices: Option<Choices>, filter: Option<&str>, escape: Escape) -> ChoiceList {
    ChoiceList {
        choices,
        filter: filter.map(str::to_string),
        legend: list_legend()
            .key(ENTER, PromptAct::Answer.said())
            .key(ESC, escape.does()),
    }
}

/// What `init -i` asks a project's name with, and a setup labels its row with.
pub const PROJECT_NAME: &str = "name";

/// What `init -i` asks a project's base branch with, and a setup labels its row with.
pub const BASE_BRANCH: &str = "base";

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

/// What a role's model is asked with, `init -i` asking it under the adapter it runs on
/// ([`runs_on`]) and a setup labelling the box it is typed in with it.
pub const MODEL: &str = "model";

/// What a model left empty comes to: the role is declared on its adapter alone, whose CLI
/// runs the model it runs by default.
pub const NONE_LEAVES_IT_TO_THE_ADAPTER: &str = "enter with none leaves it to the adapter";

/// What a line typed in place under a row comes to where it is left empty: `kept`, what stands
/// there, stays, as a setup's name and base do.
pub fn none_keeps(kept: &str) -> String {
    format!("enter with none keeps `{kept}`")
}

/// What `init -i` asks a role's model with, under the adapter it runs on: what is asked,
/// and what a line left empty comes to.
pub fn model_asked() -> Line {
    Line::new()
        .push(Tone::Strong, MODEL)
        .push(Tone::Muted, format!(" ({NONE_LEAVES_IT_TO_THE_ADAPTER})"))
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
