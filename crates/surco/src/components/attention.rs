//! What needs a person, and how they answer it: the one row a screen gives to
//! what asks for a person, wherever they are.

use super::{Binding, Keys};

/// What needs a person, and how they answer it, on any screen. Not a
/// [`Concern`](super::Concern), which warns: this asks.
///
/// A surface says the answer where a row cut short keeps it: a command
/// before what is asked, since a person who lost what it is about still has
/// the command to run, and a key after it, which what is asked gives way to.
/// An attention answered at another terminal says only what is asked.
pub struct Attention {
    /// What needs the person, when a screen shows more than one: a run's
    /// handle.
    pub subject: Option<String>,
    /// What is asked of them.
    pub asks: String,
    pub answer: Answer,
}

/// How a person answers what needs them.
pub enum Answer {
    /// Typed at a shell.
    Command(String),
    /// Pressed where they are.
    Key(Binding),
    /// Given where it was asked, at another terminal: nothing here answers
    /// it.
    Elsewhere,
}

impl Attention {
    /// What `asks`, answered by typing `command` at a shell.
    pub fn command(asks: impl Into<String>, command: impl Into<String>) -> Self {
        Attention {
            subject: None,
            asks: asks.into(),
            answer: Answer::Command(command.into()),
        }
    }

    /// What `asks`, answered by pressing `keys` where the person is, which
    /// does `does`.
    pub fn key(asks: impl Into<String>, keys: impl Into<String>, does: impl Into<String>) -> Self {
        Attention {
            subject: None,
            asks: asks.into(),
            answer: Answer::Key(Binding {
                keys: Keys::Typed(keys.into()),
                does: does.into(),
            }),
        }
    }

    /// What `asks`, answered where it was asked, at another terminal.
    pub fn elsewhere(asks: impl Into<String>) -> Self {
        Attention {
            subject: None,
            asks: asks.into(),
            answer: Answer::Elsewhere,
        }
    }

    /// This attention about `subject`: what needs the person, when a screen
    /// shows more than one.
    pub fn of(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }
}
