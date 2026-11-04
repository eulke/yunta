//! The colors a page is dressed in: the palette's, on each ground, as tokens
//! the style reads, and the few only a page has.

use crate::line::{Fill, Syntax, Tone};
use crate::paint::Ground;
use crate::palette::{self, Rgb};

/// The palette's tones, by the name the style reads.
const TONES: [(&str, Tone); 18] = [
    ("strong", Tone::Strong),
    ("muted", Tone::Muted),
    ("faint", Tone::Faint),
    ("done", Tone::Done),
    ("failed", Tone::Failed),
    ("running", Tone::Running),
    ("needs", Tone::NeedsYou),
    ("caution", Tone::Caution),
    ("added-text", Tone::Added),
    ("removed-text", Tone::Removed),
    ("kw", Tone::Code(Syntax::Keyword)),
    ("ty", Tone::Code(Syntax::Type)),
    ("fn", Tone::Code(Syntax::Function)),
    ("st", Tone::Code(Syntax::Text)),
    ("nu", Tone::Code(Syntax::Number)),
    ("cm", Tone::Code(Syntax::Comment)),
    ("at", Tone::Code(Syntax::Attribute)),
    ("ma", Tone::Code(Syntax::Macro)),
];

/// The palette's fills, by the name the style reads.
const FILLS: [(&str, Fill); 4] = [
    ("tint", Fill::Tint),
    ("code", Fill::Code),
    ("added", Fill::Added),
    ("removed", Fill::Removed),
];

/// The tokens of a page: a light ground, and a dark one where the reader's
/// system asks for it.
pub(super) fn tokens() -> String {
    format!(
        ":root{{color-scheme:light;{}}}\n@media (prefers-color-scheme:dark){{:root{{color-scheme:dark;{}}}}}",
        set(Ground::Light),
        set(Ground::Dark),
    )
}

/// Every token on `ground`, each written `--name:#rrggbb;`: the page's own
/// colors, then each tone and fill the palette colors, then the ink of a pill.
fn set(ground: Ground) -> String {
    let page = palette::page(ground)
        .into_iter()
        .map(|(name, color)| token(name, color));
    let tones = TONES
        .iter()
        .filter_map(|(name, tone)| palette::tone(ground, *tone).map(|color| token(name, color)));
    let fills = FILLS
        .iter()
        .filter_map(|(name, fill)| palette::fill(ground, *fill).map(|color| token(name, color)));
    let pill = std::iter::once(token("pill-ink", palette::pill_ink(ground)));
    page.chain(tones).chain(fills).chain(pill).collect()
}

/// One token, `--name:#rrggbb;`, for `color`.
fn token(name: &str, color: Rgb) -> String {
    format!("--{name}:{};", hex(color))
}

/// `color` as a web color: `#` and its red, green and blue in two lowercase
/// hex digits each.
fn hex(Rgb(red, green, blue): Rgb) -> String {
    format!("#{red:02x}{green:02x}{blue:02x}")
}
