//! A screen that stays open, as a page: the blocks it is made of, the ribbon
//! at the top of a run and the keys at its foot as buttons a desktop can wire,
//! and the whole screen in the elements a page lays out in a grid.

use super::blocks::{draw_all, stroke, Reader};
use super::text::{command, esc, mark, words};
use super::GLYPHS;
use crate::components::{Answer, Attention, Binding, Legend, Region, Ribbon, Screen};
use crate::glyphs::Mark;
use crate::words::Standing;

/// Each node's name over its stroke of the furrow, in the class of its word;
/// every name with its word is said in the element's label. Nothing is drawn
/// for no nodes.
pub(super) fn ribbon(ribbon: &Ribbon) -> String {
    if ribbon.stretches.is_empty() {
        return String::new();
    }
    // A space between two stretches keeps their names apart as words; a flex
    // row draws none of it.
    let stretches: String = ribbon
        .stretches
        .iter()
        .map(|stretch| {
            format!(
                r#"<span class="stretch"><span class="name">{}</span>{}</span>"#,
                esc(&stretch.name),
                stroke(stretch.standing.mark())
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let said: Vec<String> = ribbon
        .stretches
        .iter()
        .map(|stretch| format!("{} {}", stretch.name, stretch.standing.word()))
        .collect();
    format!(
        r#"<div class="ribbon" role="img" aria-label="{}">{stretches}</div>"#,
        esc(&said.join(", "))
    )
}

/// A legend as a row of buttons, each carrying its key as a program names it,
/// beside what it does: inert on a static page, for whatever wires it.
pub(super) fn legend(legend: &Legend) -> String {
    if legend.is_empty() {
        return String::new();
    }
    let bindings: Vec<String> = legend.0.iter().map(key).collect();
    format!(r#"<div class="legend">{}</div>"#, bindings.join(" "))
}

/// One key as a button carrying it as a program names it, beside what it
/// does.
fn key(binding: &Binding) -> String {
    format!(
        r#"<button type="button" class="key" data-key="{}">{}</button> {}"#,
        esc(binding.keys.named()),
        esc(binding.keys.spelled(GLYPHS)),
        esc(&binding.does)
    )
}

/// An attention as the page's status: the mark and `needs you`, its subject
/// in bold, and the answer, a command to copy before what is asked or a key's
/// button after it, or what is asked alone when it is answered at another
/// terminal. A page tints it, as a screen tints its row.
pub(super) fn attention(attention: &Attention) -> String {
    if attention.asks.trim().is_empty() {
        return String::new();
    }
    let word = Standing::NeedsYou.word();
    let head = match &attention.subject {
        None => format!("{} {word}: ", mark(Mark::NeedsYou)),
        Some(subject) => format!("{} <b>{}</b> {word}: ", mark(Mark::NeedsYou), esc(subject)),
    };
    let asks = words(&attention.asks);
    let said = match &attention.answer {
        Answer::Command(typed) => format!(
            r#"{head}{} <span class="asks">({asks})</span>"#,
            command(typed)
        ),
        Answer::Key(binding) => format!("{head}{asks} {}", key(binding)),
        Answer::Elsewhere => format!("{head}{asks}"),
    };
    format!(r#"<p class="attention" role="status">{said}</p>"#)
}

/// A screen as a page's elements, each on a line of its own: where the person
/// is and what is counted in its header, what it is about, the regions in its
/// `main`, the one the keys act on current, what needs the person and the keys
/// in its footer, and what is open over it as a dialog.
pub(super) fn screen(screen: &Screen) -> String {
    let mut reader = Reader::headed();
    let mut lines = vec![r#"<div class="screen">"#.to_string()];
    lines.extend(place(screen));
    if let Some(about) = &screen.about {
        let drawn = draw_all(&mut reader, about.blocks(), 0);
        lines.push(format!("<div class=\"about\">\n{drawn}\n</div>"));
    }
    lines.push(r#"<main class="regions">"#.to_string());
    if let Some(side) = &screen.side {
        lines.push(region(&mut reader, &side.region, "side", side.focused));
    }
    let main = &screen.main;
    lines.push(region(&mut reader, main, "main", screen.main_focused()));
    lines.push("</main>".to_string());
    let act: Vec<String> = screen
        .attention
        .iter()
        .map(attention)
        .chain(std::iter::once(legend(&screen.legend)))
        .filter(|drawn| !drawn.is_empty())
        .collect();
    if !act.is_empty() {
        lines.push(format!(
            "<footer class=\"act\">\n{}\n</footer>",
            act.join("\n")
        ));
    }
    if let Some(overlay) = &screen.overlay {
        let title = esc(&overlay.title);
        lines.push(format!(
            r#"<dialog open aria-label="{title}"><h2>{title}</h2>{}{}</dialog>"#,
            draw_all(&mut reader, overlay.doc.blocks(), 2),
            legend(&overlay.legend)
        ));
    }
    lines.push("</div>".to_string());
    lines.join("\n")
}

/// Where the person is, the outermost place in bold, and how many runs stand
/// at each word that asks for a look, each count after its mark. Nothing for a
/// screen that says neither.
fn place(screen: &Screen) -> Option<String> {
    let within = format!(r#"<span class="within"> {} </span>"#, GLYPHS.within());
    let trail: Vec<String> = screen
        .trail
        .iter()
        .enumerate()
        .map(|(at, place)| match at {
            0 => format!("<b>{}</b>", esc(place)),
            _ => esc(place),
        })
        .collect();
    let trail = (!trail.is_empty()).then(|| {
        format!(
            r#"<nav class="trail" aria-label="where you are">{}</nav>"#,
            trail.join(&within)
        )
    });
    let sep = format!(r#"<span class="sep"> {} </span>"#, GLYPHS.sep());
    let counts: Vec<String> = screen
        .tally
        .iter()
        .map(|count| format!("{} {}", mark(count.standing.mark()), esc(&count.said())))
        .collect();
    let tally = (!counts.is_empty())
        .then(|| format!(r#"<span class="tally">{}</span>"#, counts.join(&sep)));
    let said: Vec<String> = trail.into_iter().chain(tally).collect();
    (!said.is_empty()).then(|| format!(r#"<header class="place">{}</header>"#, said.join(" ")))
}

/// A region as a section in the class of its `part`, its title the heading
/// over its document, current when the keys act on it.
fn region(reader: &mut Reader, region: &Region, part: &str, focused: bool) -> String {
    let (label, heading) = match &region.title {
        Some(title) => (
            format!(r#" aria-label="{}""#, esc(title)),
            format!("<h3>{}</h3>", esc(title)),
        ),
        None => (String::new(), String::new()),
    };
    let current = match focused {
        true => r#" aria-current="true""#,
        false => "",
    };
    format!(
        r#"<section class="region {part}"{label}{current}>{heading}{}</section>"#,
        draw_all(reader, region.doc.blocks(), 2)
    )
}
