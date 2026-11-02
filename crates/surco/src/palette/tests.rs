use super::*;

/// The ground each palette is tuned on: what a dark terminal and a light
/// one draw behind their text.
const GROUND: Tuned = Tuned {
    dark: Rgb(21, 19, 26),
    light: Rgb(251, 250, 246),
};

/// Every tone the palette colors outside code: what a line is about, what a
/// reader skims past, each state, and each change. The words of code are in
/// `SYNTAX`, where a comment, which recedes, has a floor of its own.
const COLORED: [Tone; 9] = [
    Tone::Strong,
    Tone::Muted,
    Tone::Done,
    Tone::Failed,
    Tone::Running,
    Tone::NeedsYou,
    Tone::Caution,
    Tone::Added,
    Tone::Removed,
];

/// Every kind of word code is colored as.
const SYNTAX: [Syntax; 8] = [
    Syntax::Keyword,
    Syntax::Type,
    Syntax::Function,
    Syntax::Text,
    Syntax::Number,
    Syntax::Comment,
    Syntax::Attribute,
    Syntax::Macro,
];

/// The contrast between two colors, from 1 to 21, as the web's
/// accessibility guidelines measure it: how far apart their relative
/// luminances sit.
fn contrast(one: Rgb, other: Rgb) -> f64 {
    let (one, other) = (luminance(one), luminance(other));
    (one.max(other) + 0.05) / (one.min(other) + 0.05)
}

/// How bright `color` reads to the eye, from 0 for black to 1 for white.
fn luminance(Rgb(red, green, blue): Rgb) -> f64 {
    let linear = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        match value <= 0.04045 {
            true => value / 12.92,
            false => ((value + 0.055) / 1.055).powf(2.4),
        }
    };
    0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
}

/// The measure itself, on pairs whose contrast is known: black on white is
/// 21:1, and a color against itself is 1:1.
#[test]
fn contrast_measures_black_on_white_as_twenty_one_to_one() {
    let (black, white) = (Rgb(0, 0, 0), Rgb(255, 255, 255));
    assert!((contrast(black, white) - 21.0).abs() < 1e-9);
    assert!((contrast(white, white) - 1.0).abs() < 1e-9);
}

/// Every tone in `COLORED` reads on the ground it is tuned for, and on the
/// tint and the code ground a whole line sits on.
#[test]
fn every_colored_tone_reads_on_its_ground_and_on_a_line_fill() {
    for ground in [Ground::Dark, Ground::Light] {
        for colored in COLORED {
            let color = tone(ground, colored).expect("a colored tone has a color");
            let on_ground = contrast(color, GROUND.on(ground));
            assert!(on_ground >= 4.3, "{ground:?} / {colored:?}: {on_ground:.2}");
            for line in [Fill::Tint, Fill::Code] {
                let behind = fill(ground, line).expect("a line fill has a color");
                let on_fill = contrast(color, behind);
                assert!(
                    on_fill >= 3.8,
                    "{ground:?} / {colored:?} / {line:?}: {on_fill:.2}"
                );
            }
        }
    }
}

/// A word a change marks reads on the fill that marks it.
#[test]
fn a_changed_word_reads_on_the_fill_that_marks_it() {
    for ground in [Ground::Dark, Ground::Light] {
        for (changed, marked) in [(Tone::Added, Fill::Added), (Tone::Removed, Fill::Removed)] {
            let color = tone(ground, changed).expect("a change's tone has a color");
            let behind = fill(ground, marked).expect("a change's fill has a color");
            let on_fill = contrast(color, behind);
            assert!(on_fill >= 3.8, "{ground:?} / {changed:?}: {on_fill:.2}");
        }
    }
}

/// A pill's word reads on the color of every tone a pill can be drawn in.
#[test]
fn a_pills_word_reads_on_the_color_of_every_tone() {
    for ground in [Ground::Dark, Ground::Light] {
        for colored in COLORED {
            let color = tone(ground, colored).expect("a colored tone has a color");
            let on_pill = contrast(pill_ink(ground), color);
            assert!(on_pill >= 4.5, "{ground:?} / {colored:?}: {on_pill:.2}");
        }
    }
}

/// Every word of code reads on its ground and on the code ground: a comment,
/// which recedes, at 2.5:1 against both, every other word at 4.3:1 against
/// its ground and 3.8:1 against the code ground.
#[test]
fn every_word_of_code_reads_on_its_ground_and_on_the_code_ground() {
    for ground in [Ground::Dark, Ground::Light] {
        let code = fill(ground, Fill::Code).expect("the code ground has a color");
        for syntax in SYNTAX {
            let color = tone(ground, Tone::Code(syntax)).expect("a word of code has a color");
            let (floor_on_ground, floor_on_code) = match syntax {
                Syntax::Comment => (2.5, 2.5),
                _ => (4.3, 3.8),
            };
            let on_ground = contrast(color, GROUND.on(ground));
            let on_code = contrast(color, code);
            assert!(
                on_ground >= floor_on_ground,
                "{ground:?} / {syntax:?}: {on_ground:.2}"
            );
            assert!(
                on_code >= floor_on_code,
                "{ground:?} / {syntax:?} on code: {on_code:.2}"
            );
        }
    }
}
