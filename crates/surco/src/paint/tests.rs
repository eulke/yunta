use super::*;
use crate::glyphs::Mark;
use crate::line::Syntax;
use std::path::Path;

/// Every tone a line can carry, the command included.
const TONES: [Tone; 20] = [
    Tone::Plain,
    Tone::Strong,
    Tone::Command,
    Tone::Muted,
    Tone::Faint,
    Tone::Done,
    Tone::Failed,
    Tone::Running,
    Tone::NeedsYou,
    Tone::Caution,
    Tone::Added,
    Tone::Removed,
    Tone::Code(Syntax::Keyword),
    Tone::Code(Syntax::Type),
    Tone::Code(Syntax::Function),
    Tone::Code(Syntax::Text),
    Tone::Code(Syntax::Number),
    Tone::Code(Syntax::Comment),
    Tone::Code(Syntax::Attribute),
    Tone::Code(Syntax::Macro),
];

/// Every fill a span or a line can carry.
const FILLS: [Fill; 5] = [
    Fill::Tint,
    Fill::Code,
    Fill::Added,
    Fill::Removed,
    Fill::Pill,
];

/// The 24-bit layer on a dark ground, without links.
const DARK_24: Paint = Paint::Color {
    layer: Layer::Truecolor(Ground::Dark),
    links: false,
};

/// The 24-bit layer on a light ground, without links.
const LIGHT_24: Paint = Paint::Color {
    layer: Layer::Truecolor(Ground::Light),
    links: false,
};

/// Every tone around texts that carry what a line can: spaces, glyphs,
/// backticks, a cell-wide character, an empty word.
#[test]
fn plain_text_equals_painted_text_with_escapes_stripped() {
    let texts = [
        "",
        "needs you",
        "✓ finished",
        "`yunta resume 7E5PH4`",
        "名前 · 3s",
    ];
    for (at, first) in TONES.iter().enumerate() {
        for second in TONES.iter().skip(at) {
            for text in texts {
                let line = Line::new()
                    .push(*first, text)
                    .plain(" · ")
                    .push(*second, text);
                assert_eq!(
                    strip_sgr(&Paint::ANSI16.line(&line)),
                    Paint::Plain.line(&line),
                    "{first:?} / {second:?} / {text:?}"
                );
            }
        }
    }
}

#[test]
fn no_color_wins_over_auto_but_not_over_always() {
    let no_color = ColorPolicy {
        no_color: true,
        ..ColorPolicy::default()
    };
    assert_eq!(no_color.paint(true), Paint::Plain);
    let always = ColorPolicy {
        when: ColorWhen::Always,
        ..no_color
    };
    assert_eq!(always.paint(false), Paint::ANSI16);
}

#[test]
fn the_command_line_decides_before_the_look() {
    let never = ColorPolicy {
        when: ColorWhen::Never,
        look: Some(LookChoice::Truecolor),
        force: true,
        ..ColorPolicy::default()
    };
    assert_eq!(never.paint(true), Paint::Plain);
    let always = ColorPolicy {
        when: ColorWhen::Always,
        look: Some(LookChoice::Plain),
        no_color: true,
        ..ColorPolicy::default()
    };
    assert_eq!(always.paint(false), Paint::ANSI16);
}

#[test]
fn a_look_of_plain_is_plain_on_a_terminal() {
    let plain = ColorPolicy {
        look: Some(LookChoice::Plain),
        force: true,
        truecolor: true,
        ..ColorPolicy::default()
    };
    assert_eq!(plain.paint(true), Paint::Plain);
}

#[test]
fn no_color_is_plain_whatever_the_look() {
    let no_color = ColorPolicy {
        no_color: true,
        look: Some(LookChoice::Truecolor),
        truecolor: true,
        ..ColorPolicy::default()
    };
    assert_eq!(no_color.paint(true), Paint::Plain);
}

#[test]
fn clicolor_force_paints_a_pipe() {
    let forced = ColorPolicy {
        force: true,
        ..ColorPolicy::default()
    };
    assert_eq!(forced.paint(false), Paint::ANSI16);
}

#[test]
fn color_is_decided_per_stream() {
    let auto = ColorPolicy::default();
    assert_eq!(auto.paint(true), Paint::ANSI16);
    assert_eq!(auto.paint(false), Paint::Plain);
    let dumb = ColorPolicy {
        dumb: true,
        ..ColorPolicy::default()
    };
    assert_eq!(dumb.paint(true), Paint::Plain);
    let off = ColorPolicy {
        off: true,
        ..ColorPolicy::default()
    };
    assert_eq!(off.paint(true), Paint::Plain);
}

#[test]
fn a_look_names_the_layer_over_what_the_terminal_draws() {
    let sixteen = ColorPolicy {
        look: Some(LookChoice::Ansi16),
        truecolor: true,
        ..ColorPolicy::default()
    };
    assert_eq!(sixteen.paint(true), Paint::ANSI16);
    let truecolor = ColorPolicy {
        look: Some(LookChoice::Truecolor),
        ground: Ground::Light,
        ..ColorPolicy::default()
    };
    assert_eq!(truecolor.paint(true), LIGHT_24);
}

#[test]
fn a_terminal_that_draws_24_bits_gets_them_on_its_ground() {
    let dark = ColorPolicy {
        truecolor: true,
        ..ColorPolicy::default()
    };
    assert_eq!(dark.paint(true), DARK_24);
    let light = ColorPolicy {
        truecolor: true,
        ground: Ground::Light,
        ..ColorPolicy::default()
    };
    assert_eq!(light.paint(true), LIGHT_24);
    assert_eq!(dark.paint(false), Paint::Plain, "a pipe gets none");
}

#[test]
fn links_are_drawn_where_the_terminal_announces_them_or_a_reader_forces_them() {
    let with = |links| ColorPolicy {
        links,
        ..ColorPolicy::default()
    };
    assert_eq!(with(Links::Announced).paint(true), Paint::LINKED);
    assert_eq!(with(Links::Announced).paint(false), Paint::Plain);
    assert_eq!(with(Links::Unknown).paint(true), Paint::ANSI16);
    assert_eq!(with(Links::Refused).paint(true), Paint::ANSI16);
    let forced = ColorPolicy {
        when: ColorWhen::Always,
        links: Links::Forced,
        ..ColorPolicy::default()
    };
    assert_eq!(forced.paint(false), Paint::LINKED, "a pipe asked for both");
    let no_color = ColorPolicy {
        no_color: true,
        links: Links::Forced,
        ..ColorPolicy::default()
    };
    assert_eq!(no_color.paint(true), Paint::Plain, "no color, no escapes");
}

#[test]
fn links_are_drawn_in_any_layer_with_color() {
    let linked = ColorPolicy {
        truecolor: true,
        links: Links::Announced,
        ..ColorPolicy::default()
    };
    assert_eq!(
        linked.paint(true),
        Paint::Color {
            layer: Layer::Truecolor(Ground::Dark),
            links: true,
        }
    );
}

#[test]
fn ground_of_reads_the_background_the_colorfgbg_names() {
    assert_eq!(ground_of(Some("15;0")), Ground::Dark);
    assert_eq!(ground_of(Some("15;8")), Ground::Dark);
    assert_eq!(ground_of(Some("0;15")), Ground::Light);
    assert_eq!(ground_of(Some("0;7")), Ground::Light);
    assert_eq!(ground_of(Some("0;9")), Ground::Light);
    assert_eq!(ground_of(Some("default;default")), Ground::Dark);
    assert_eq!(ground_of(None), Ground::Dark);
}

#[test]
fn look_of_names_the_three_layers_and_no_other_value() {
    assert_eq!(look_of(Some("plain")), Some(LookChoice::Plain));
    assert_eq!(look_of(Some(" 16 ")), Some(LookChoice::Ansi16));
    assert_eq!(look_of(Some("24")), Some(LookChoice::Truecolor));
    assert_eq!(look_of(Some("truecolor")), None);
    assert_eq!(look_of(None), None);
}

/// Every layer writes the same words for every tone, on every fill a span
/// or a line carries: with its escapes stripped, a painted line reads as
/// its text.
#[test]
fn every_layer_paints_the_same_words_for_every_tone_and_fill() {
    for paint in [Paint::Plain, Paint::ANSI16, DARK_24, LIGHT_24] {
        for tone in TONES {
            for fill in FILLS {
                let spans = Line::new()
                    .push_filled(tone, fill, "needs you")
                    .plain(": ")
                    .push(tone, "3s");
                for line in [spans.clone(), spans.filled(fill)] {
                    assert_eq!(
                        strip_sgr(&paint.line(&line)),
                        line.text(),
                        "{paint:?} / {tone:?} / {fill:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_done_span_on_a_dark_ground_is_drawn_in_its_24_bit_color() {
    assert_eq!(
        DARK_24.word(Tone::Done, "done"),
        "\x1b[38;2;143;206;155mdone\x1b[0m"
    );
    assert_eq!(DARK_24.word(Tone::Command, "yunta"), "\x1b[1myunta\x1b[0m");
}

#[test]
fn a_fill_on_a_light_ground_is_its_24_bit_background() {
    let line = Line::new().push_filled(Tone::Plain, Fill::Code, "fn main()");
    assert_eq!(
        LIGHT_24.line(&line),
        "\x1b[48;2;244;242;238mfn main()\x1b[0m"
    );
}

#[test]
fn an_ansi16_span_never_paints_a_fill() {
    let line = Line::new()
        .push_filled(Tone::Done, Fill::Added, "done")
        .filled(Fill::Code);
    assert_eq!(Paint::ANSI16.line(&line), "\x1b[32mdone\x1b[0m");
}

#[test]
fn a_filled_truecolor_line_is_erased_to_its_edge_with_no_space() {
    let line = Line::new().push(Tone::Plain, "text").filled(Fill::Code);
    assert_eq!(
        DARK_24.line(&line),
        "\x1b[48;2;26;24;31mtext\x1b[0m\x1b[48;2;26;24;31m\x1b[K\x1b[0m"
    );
}

#[test]
fn a_pill_is_its_tone_on_pill_ink_in_bold() {
    let line = Line::new().push_filled(Tone::Done, Fill::Pill, "done");
    assert_eq!(
        DARK_24.line(&line),
        "\x1b[1;38;2;28;18;34;48;2;143;206;155mdone\x1b[0m"
    );
    assert_eq!(Paint::ANSI16.line(&line), "\x1b[32mdone\x1b[0m");
}

/// A pill in a tone the palette leaves to the terminal has no color to
/// paint its background in, so it is its word on the line's fill.
#[test]
fn a_pill_with_no_color_of_its_own_is_its_word_on_the_line_fill() {
    let line = Line::new()
        .filled(Fill::Code)
        .push_filled(Tone::Plain, Fill::Pill, "x")
        .push_filled(Tone::Command, Fill::Pill, "y");
    assert_eq!(
        DARK_24.line(&line),
        [
            "\x1b[48;2;26;24;31mx\x1b[0m",
            "\x1b[1;48;2;26;24;31my\x1b[0m",
            "\x1b[48;2;26;24;31m\x1b[K\x1b[0m",
        ]
        .concat()
    );
}

/// A line filled as a pill has no background of its own, so it draws none.
#[test]
fn a_line_filled_as_a_pill_draws_no_background() {
    let line = Line::new().push(Tone::Done, "a").filled(Fill::Pill);
    assert_eq!(DARK_24.line(&line), "\x1b[38;2;143;206;155ma\x1b[0m");
}

/// A line's fill is behind every span: each span's reset ends its own
/// colors and leaves the background for the next span to draw on.
#[test]
fn a_line_fill_is_behind_every_span_across_each_reset() {
    let line = Line::new()
        .filled(Fill::Code)
        .push(Tone::Done, "a")
        .plain(" ")
        .push(Tone::Failed, "b");
    assert_eq!(
        DARK_24.line(&line),
        [
            "\x1b[38;2;143;206;155;48;2;26;24;31ma\x1b[0m",
            "\x1b[48;2;26;24;31m \x1b[0m",
            "\x1b[38;2;238;139;121;48;2;26;24;31mb\x1b[0m",
            "\x1b[48;2;26;24;31m\x1b[K\x1b[0m",
        ]
        .concat()
    );
}

#[test]
fn a_link_is_written_where_links_are_drawn_and_nowhere_plain() {
    let line = Line::new().path("a.rs", Path::new("/src/a.rs"));
    let linked = "\x1b]8;;file:///src/a.rs\x1b\\a.rs\x1b]8;;\x1b\\";
    assert_eq!(Paint::LINKED.line(&line), linked);
    assert_eq!(
        Paint::Color {
            layer: Layer::Truecolor(Ground::Dark),
            links: true,
        }
        .line(&line),
        linked
    );
    assert_eq!(Paint::ANSI16.line(&line), "a.rs");
    assert_eq!(Paint::Plain.line(&line), "a.rs");
}

#[test]
fn strip_sgr_takes_out_every_escape_a_line_writes() {
    let painted = "\x1b]8;;file:///a\x1b\\a\x1b[1;38;2;1;2;3mb\x1b[0m\x1b[K\x1b]8;;\x1b\\";
    assert_eq!(strip_sgr(painted), "ab");
}

/// An erase ends in `K`, not `m`, so the text after it is kept rather than
/// taken with it up to the next `m`.
#[test]
fn strip_sgr_keeps_the_text_an_erase_is_followed_by() {
    assert_eq!(strip_sgr("\x1b[Kabc\x1b[0m"), "abc");
}

/// What a change puts into code is green and what it takes out red,
/// the colors git paints a word diff in.
#[test]
fn an_added_and_a_removed_word_are_painted_apart() {
    assert_eq!(
        Paint::ANSI16.word(Tone::Added, "{+b+}"),
        "\x1b[32m{+b+}\x1b[0m"
    );
    assert_eq!(
        Paint::ANSI16.word(Tone::Removed, "[-a-]"),
        "\x1b[31m[-a-]\x1b[0m"
    );
    assert_eq!(Paint::Plain.word(Tone::Added, "{+b+}"), "{+b+}");
}

#[test]
fn a_person_and_a_caution_are_painted_apart() {
    assert_ne!(
        Paint::ANSI16.word(Tone::NeedsYou, "x"),
        Paint::ANSI16.word(Tone::Caution, "x")
    );
    assert_eq!(Tone::of(Mark::NeedsYou), Tone::NeedsYou);
    assert_eq!(Tone::of(Mark::Reroute), Tone::Caution);
}

#[test]
fn a_title_is_set_after_keeping_the_one_there_and_carries_no_control() {
    assert_eq!(
        title_saved("◆ needs\tyou\x1b]2;x\x07"),
        "\x1b[22;2t\x1b]2;◆ needs you ]2;x \x1b\\"
    );
}

/// A word of code is plain in the sixteen colors, and its own color in 24
/// bits: a keyword on a dark ground is written by hand below.
#[test]
fn a_word_of_code_is_plain_in_sixteen_colors_and_colored_in_24_bits() {
    let keyword = Tone::Code(Syntax::Keyword);
    assert_eq!(Paint::ANSI16.word(keyword, "fn"), "fn");
    assert_eq!(
        DARK_24.word(keyword, "fn"),
        "\x1b[38;2;147;169;215mfn\x1b[0m"
    );
    assert_eq!(
        LIGHT_24.word(keyword, "fn"),
        "\x1b[38;2;61;90;166mfn\x1b[0m"
    );
}
