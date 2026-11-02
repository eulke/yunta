use super::*;
use crate::paint::sgr;
use clap::builder::styling::Style;

/// The seven roles clap draws in, each with the tone a page paints it in.
fn roles_and_tones(styles: &clap::builder::Styles) -> [(&Style, Tone); 7] {
    [
        (styles.get_header(), Tone::Muted),
        (styles.get_usage(), Tone::Muted),
        (styles.get_literal(), Tone::Strong),
        (styles.get_placeholder(), Tone::Muted),
        (styles.get_error(), Tone::Failed),
        (styles.get_valid(), Tone::Done),
        (styles.get_invalid(), Tone::Caution),
    ]
}

#[test]
fn clap_draws_each_tone_in_the_escape_a_page_paints_it_in() {
    let styles = clap_styles(Paint::ANSI16);
    for (style, tone) in roles_and_tones(&styles) {
        assert_eq!(
            style.render().to_string(),
            format!("\x1b[{}m", sgr(tone).unwrap_or_default()),
            "{tone:?}"
        );
    }
}

/// Each role renders in its tone's 24-bit color on either ground, bold where
/// the sixteen colors are bold: the escapes are written out by hand.
#[test]
fn clap_draws_each_tone_in_its_24_bit_color_on_either_ground() {
    let dark = [
        "\x1b[38;2;142;136;127m",
        "\x1b[38;2;142;136;127m",
        "\x1b[1m\x1b[38;2;255;255;255m",
        "\x1b[38;2;142;136;127m",
        "\x1b[38;2;238;139;121m",
        "\x1b[38;2;143;206;155m",
        "\x1b[38;2;232;192;99m",
    ];
    let light = [
        "\x1b[38;2;123;117;111m",
        "\x1b[38;2;123;117;111m",
        "\x1b[1m\x1b[38;2;14;12;18m",
        "\x1b[38;2;123;117;111m",
        "\x1b[38;2;194;65;47m",
        "\x1b[38;2;45;134;73m",
        "\x1b[38;2;148;101;0m",
    ];
    for (ground, escapes) in [(Ground::Dark, dark), (Ground::Light, light)] {
        let styles = clap_styles(Paint::Color {
            layer: Layer::Truecolor(ground),
            links: false,
        });
        for ((style, tone), escape) in roles_and_tones(&styles).into_iter().zip(escapes) {
            assert_eq!(style.render().to_string(), escape, "{ground:?} / {tone:?}");
        }
    }
}

/// A plain paint writes no escape: each role renders as nothing.
#[test]
fn clap_draws_no_escape_on_a_plain_paint() {
    let styles = clap_styles(Paint::Plain);
    for (style, tone) in roles_and_tones(&styles) {
        assert_eq!(style.render().to_string(), "", "{tone:?}");
    }
}
