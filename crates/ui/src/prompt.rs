//! The prompts a person answers stroke by stroke, apart from the medium that holds
//! them: where each stands, and what a stroke does there. Each draws as one Surco
//! block, worded by render's `prompt`, which a terminal holds at its height with the
//! cursor and a page or a window draws as a document.
//!
//! A surface reads its own input as [`Stroke`]s, hands each to the prompt it holds,
//! and reads the [`Press`] back: it stays, it moved on, it was answered, left or
//! stopped. What a surface laid out of a prompt that scrolls comes back to it through
//! [`Answerable::laid`], so drawing changes nothing.

mod answerable;
mod deciding;
mod decision_card;
mod decision_put;
mod line_asked;
mod picker;
mod plan_desk;
mod press;
mod round;
mod stroke;
mod typed_line;

pub use answerable::Answerable;
pub use deciding::{Deciding, InFront, Opens};
pub use decision_card::{DecisionCard, PickedOption, Reply};
pub use decision_put::DecisionPut;
pub use line_asked::LineAsked;
pub use picker::{Alternative, Picker};
pub use plan_desk::{Asked, Left, PlanDesk};
pub use press::Press;
pub use round::{Round, SettledAnswers};
pub use stroke::Stroke;
pub use typed_line::{Edit, Typed, TypedLine};
