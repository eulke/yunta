//! The colors a 24-bit stream draws in, tuned for a dark ground and a light
//! one: the roles the sixteen colors stand for, the fills, and the ink of a
//! pill.
//!
//! Every color keeps a floor of contrast, measured as the web's accessibility
//! guidelines measure it, and the tests hold each floor: a tone the palette
//! colors keeps 4.3:1 against the ground it is tuned for and 3.8:1 against the
//! tint and the code ground a whole line sits on; a word a change marks keeps
//! 3.8:1 against the fill that marks it; and a pill's ink keeps 4.5:1 against
//! the color of every tone. A color retuned to taste keeps these, or a test
//! names the one it lost.

use crate::line::{Fill, Tone};
use crate::paint::Ground;

/// A color in 24 bits: red, green and blue, each from 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb(pub u8, pub u8, pub u8);

/// One color for each ground: the one tuned for a dark terminal and the one
/// tuned for a light terminal.
#[derive(Debug, Clone, Copy)]
struct Tuned {
    dark: Rgb,
    light: Rgb,
}

impl Tuned {
    /// The color on `ground`.
    fn on(self, ground: Ground) -> Rgb {
        match ground {
            Ground::Dark => self.dark,
            Ground::Light => self.light,
        }
    }
}

/// Strong text: what a line is about.
const STRONG: Tuned = Tuned {
    dark: Rgb(255, 255, 255),
    light: Rgb(14, 12, 18),
};

/// Muted text: a label, a duration or a path a reader skims past.
const MUTED: Tuned = Tuned {
    dark: Rgb(142, 136, 127),
    light: Rgb(123, 117, 111),
};

/// Done: a node or a run that finished.
const DONE: Tuned = Tuned {
    dark: Rgb(143, 206, 155),
    light: Rgb(45, 134, 73),
};

/// Failed: a node or a run that failed.
const FAILED: Tuned = Tuned {
    dark: Rgb(238, 139, 121),
    light: Rgb(194, 65, 47),
};

/// Running: something that moves on its own, or waits on a clock.
const RUNNING: Tuned = Tuned {
    dark: Rgb(121, 198, 220),
    light: Rgb(29, 124, 151),
};

/// Needs you: the one color that means a person.
const NEEDS_YOU: Tuned = Tuned {
    dark: Rgb(220, 148, 234),
    light: Rgb(158, 47, 155),
};

/// Caution: worth a reader's attention, and nothing anybody has to answer.
const CAUTION: Tuned = Tuned {
    dark: Rgb(232, 192, 99),
    light: Rgb(148, 101, 0),
};

/// Added text: a word a change puts into a line of code; on a light ground,
/// the color done takes there.
const ADDED_TEXT: Tuned = Tuned {
    dark: Rgb(147, 199, 164),
    light: Rgb(45, 134, 73),
};

/// Removed text: a word a change takes out of a line of code; on a light
/// ground, the color failed takes there.
const REMOVED_TEXT: Tuned = Tuned {
    dark: Rgb(214, 156, 140),
    light: Rgb(194, 65, 47),
};

/// The tint behind a choice that waits on a person, set apart from the page.
const TINT: Tuned = Tuned {
    dark: Rgb(36, 26, 43),
    light: Rgb(246, 233, 243),
};

/// The ground of a code block.
const CODE: Tuned = Tuned {
    dark: Rgb(26, 24, 31),
    light: Rgb(244, 242, 238),
};

/// The background of a word a change adds.
const ADDED: Tuned = Tuned {
    dark: Rgb(36, 64, 47),
    light: Rgb(223, 240, 227),
};

/// The background of a word a change takes out.
const REMOVED: Tuned = Tuned {
    dark: Rgb(70, 40, 43),
    light: Rgb(246, 222, 219),
};

/// The ink of a pill's word, on the color of its tone: a deep ink on a dark
/// ground's light colors, white on a light ground's deep ones.
const PILL_INK: Tuned = Tuned {
    dark: Rgb(28, 18, 34),
    light: Rgb(255, 255, 255),
};

/// The color `tone` is drawn in on `ground`, or None for the terminal's own
/// text color: plain text, and a command, which is bold rather than colored.
pub(crate) fn tone(ground: Ground, tone: Tone) -> Option<Rgb> {
    let color = match tone {
        Tone::Plain | Tone::Command => return None,
        Tone::Strong => STRONG,
        Tone::Muted => MUTED,
        Tone::Done => DONE,
        Tone::Failed => FAILED,
        Tone::Running => RUNNING,
        Tone::NeedsYou => NEEDS_YOU,
        Tone::Caution => CAUTION,
        Tone::Added => ADDED_TEXT,
        Tone::Removed => REMOVED_TEXT,
    };
    Some(color.on(ground))
}

/// The background `fill` paints on `ground`. A pill has no background of its
/// own: it is drawn in its word's tone, which [`tone`] gives, so this gives
/// none for it.
pub(crate) fn fill(ground: Ground, fill: Fill) -> Option<Rgb> {
    let color = match fill {
        Fill::Tint => TINT,
        Fill::Code => CODE,
        Fill::Added => ADDED,
        Fill::Removed => REMOVED,
        Fill::Pill => return None,
    };
    Some(color.on(ground))
}

/// The ink a pill's text is drawn in on `ground`.
pub(crate) fn pill_ink(ground: Ground) -> Rgb {
    PILL_INK.on(ground)
}

#[cfg(test)]
mod tests;
