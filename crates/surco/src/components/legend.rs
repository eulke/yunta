//! The keys of what is on screen, as data: what a terminal writes in a row,
//! a page as buttons, and a desktop as whatever it presses with.

use crate::glyphs::Glyphs;

/// The keys a person can press where they are, each with what it does, in the
/// order they are read. Not a [`Next`](super::Next), which says what to type at
/// a shell: a legend's keys act on the screen it stands under.
#[derive(Default, Clone)]
pub struct Legend(pub Vec<Binding>);

/// What is pressed and what it does: `enter` and `open`.
#[derive(Clone)]
pub struct Binding {
    pub keys: Keys,
    pub does: String,
}

/// What is pressed, as the person reads it: the arrows, which each glyph set
/// draws its own way, keys spelled as they are typed (`enter`, `esc`, `n`), or
/// whatever the person types.
#[derive(Clone)]
pub enum Keys {
    UpDown,
    /// The arrows that change which word of a row is chosen. Not `UpDown`, which move
    /// along a list.
    LeftRight,
    Typed(String),
    /// Whatever the person types, which filters a list or fills a name:
    /// spelled `type`. Not `Typed`, which names the keys a person presses.
    Typing,
}

impl Legend {
    /// A legend with no key yet.
    pub fn new() -> Self {
        Legend::default()
    }

    /// This legend with the arrows doing `does`.
    pub fn arrows(mut self, does: impl Into<String>) -> Self {
        self.0.push(Binding {
            keys: Keys::UpDown,
            does: does.into(),
        });
        self
    }

    /// This legend with the arrows that change the word chosen doing `does`: `←→ mode`.
    pub fn left_right(mut self, does: impl Into<String>) -> Self {
        self.0.push(Binding {
            keys: Keys::LeftRight,
            does: does.into(),
        });
        self
    }

    /// This legend with `keys`, spelled as they are typed, doing `does`.
    pub fn key(mut self, keys: impl Into<String>, does: impl Into<String>) -> Self {
        self.0.push(Binding {
            keys: Keys::Typed(keys.into()),
            does: does.into(),
        });
        self
    }

    /// This legend with whatever the person types doing `does`: `type to
    /// filter`.
    pub fn typing(mut self, does: impl Into<String>) -> Self {
        self.0.push(Binding {
            keys: Keys::Typing,
            does: does.into(),
        });
        self
    }

    /// Whether it holds no key, and so says nothing.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Keys {
    /// The keys as a person reads them in `glyphs`: the arrows in that glyph
    /// set's own, and typed keys as they are typed.
    pub fn spelled(&self, glyphs: Glyphs) -> &str {
        match self {
            Keys::UpDown => glyphs.up_down(),
            Keys::LeftRight => glyphs.left_right(),
            Keys::Typed(keys) => keys,
            Keys::Typing => "type",
        }
    }

    /// The keys as a program names them, the same in every glyph set: what a
    /// page's button carries for whatever wires it.
    pub fn named(&self) -> &str {
        match self {
            Keys::UpDown => "up-down",
            Keys::LeftRight => "left-right",
            Keys::Typed(keys) => keys,
            Keys::Typing => "typing",
        }
    }
}
