//! A screen that stays open, as a page: the blocks it is made of, the ribbon
//! at the top of a run and the keys at its foot as buttons a desktop can wire,
//! and the whole screen in the elements a page lays out in a grid.

use super::blocks::{draw_all, stroke, Reader};
use super::text::{command, esc, mark, words};
use super::GLYPHS;
use crate::components::{
    Answer, AtAGlance, Attention, Binding, KeyList, Legend, Noted, Region, Ribbon, SameAtAShell,
    Screen, Tally, WordChoice,
};
use crate::glyphs::Mark;
use crate::words::{Standing, NOTHING_NEEDS_YOU};

/// The commands that do the same at a shell, each a command a reader copies, a row each;
/// nothing for none.
pub(super) fn same_at_a_shell(same: &SameAtAShell) -> String {
    if same.0.is_empty() {
        return String::new();
    }
    let rows: String = same
        .0
        .iter()
        .map(|said| command(&crate::text::one_line(said)))
        .collect();
    format!(r#"<div class="shell">{rows}</div>"#)
}

/// A row of words, apart by the separator, the chosen one after the pointer and in bold, the
/// rest muted.
pub(super) fn word_choice(choice: &WordChoice) -> String {
    let chosen = choice.chosen_word();
    let row: Vec<String> = choice
        .words
        .iter()
        .enumerate()
        .map(|(at, word)| match at == chosen {
            true => format!("<b>{} {}</b>", GLYPHS.pointer(), words(word)),
            false => format!(r#"<span class="muted">{}</span>"#, words(word)),
        })
        .collect();
    format!("<p>{}</p>", row.join(&GLYPHS.spaced_sep()))
}

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

/// A screen's keys as a row of buttons: its own, then the far keys apart at the row's far
/// end.
fn keys(own: &Legend, anywhere: &Legend) -> String {
    if anywhere.is_empty() {
        return legend(own);
    }
    let buttons = |keys: &Legend| keys.0.iter().map(key).collect::<Vec<_>>().join(" ");
    format!(
        r#"<div class="legend">{} <span class="far">{}</span></div>"#,
        buttons(own),
        buttons(anywhere)
    )
}

/// A list of keys: its sentence, muted, then a key an item, each a button beside what it
/// does. Nothing for a list with neither.
pub(super) fn key_list(list: &KeyList) -> String {
    keys_under(list.said.as_deref(), &list.keys.0)
}

/// `keys` under `said`, muted, where there is one: a key an item, each a button beside what
/// it does. Every list of keys a page draws draws its keys so.
fn keys_under(said: Option<&str>, keys: &[Binding]) -> String {
    let said = said
        .map(|said| format!(r#"<p class="muted">{}</p>"#, words(said)))
        .unwrap_or_default();
    let keys: String = keys
        .iter()
        .map(|binding| format!("<li>{}</li>", key(binding)))
        .collect();
    match keys.is_empty() {
        true => said,
        false => format!(r#"{said}<ul class="keys">{keys}</ul>"#),
    }
}

/// A run at a glance: its facts that say something as a list of terms, each label beside its
/// words, then the acts as a list of keys under their heading, where the glance offers any.
pub(super) fn at_a_glance(glance: &AtAGlance) -> String {
    let facts: String = glance
        .said()
        .map(|(label, value)| format!("<dt>{}</dt><dd>{}</dd>", esc(label), words(value)))
        .collect();
    let facts = match facts.is_empty() {
        true => String::new(),
        false => format!(r#"<dl class="fields">{facts}</dl>"#),
    };
    let acts = glance
        .acts
        .as_ref()
        .filter(|(_, acts)| !acts.is_empty())
        .map(|(heading, acts)| keys_under(Some(heading), acts))
        .unwrap_or_default();
    format!("{facts}{acts}")
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
/// terminal or said where a line is typed. A page tints it, as a screen tints
/// its row.
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
        Answer::Elsewhere | Answer::Unkeyed => format!("{head}{asks}"),
    };
    format!(r#"<p class="attention" role="status">{said}</p>"#)
}

/// A note as the page's status, in the attention's place: its mark, then what
/// the act did. Nothing tints it, as nothing tints a screen's note.
pub(super) fn noted(noted: &Noted) -> String {
    format!(
        r#"<p class="noted" role="status">{} {}</p>"#,
        mark(noted.mark),
        words(&noted.said)
    )
}

/// A screen as a page's elements, each on a line of its own: where the person
/// is and what is counted in its header, what it is about, the regions in its
/// `main`, the one the keys act on current, what the run it is about has spent,
/// what needs the person, or in its place what their last act came to, and the
/// keys in its footer, the far keys at their row's far end, and what is open over
/// it as a dialog.
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
    let meter = screen.meter.iter().map(|meter| meter.said(GLYPHS));
    lines.extend(meter.map(|said| super::blocks::lines(&[said])));
    let said = match &screen.noted {
        Some(said) => Some(noted(said)),
        None => screen.attention.as_ref().map(attention),
    };
    let act: Vec<String> = said
        .into_iter()
        .chain(std::iter::once(keys(&screen.legend, &screen.anywhere)))
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
/// at each word that asks for a look, each count after its mark, or that nothing
/// needs the person where the runs counted need nothing. Nothing for a screen
/// that says neither.
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
    let counts: Vec<String> = match &screen.tally {
        Tally::Uncounted => Vec::new(),
        Tally::Counted(counts) if counts.is_empty() => vec![esc(NOTHING_NEEDS_YOU)],
        Tally::Counted(counts) => counts
            .iter()
            .map(|count| format!("{} {}", mark(count.standing.mark()), esc(&count.said())))
            .collect(),
    };
    let tally = (!counts.is_empty())
        .then(|| format!(r#"<span class="tally">{}</span>"#, counts.join(&sep)));
    let said: Vec<String> = trail.into_iter().chain(tally).collect();
    (!said.is_empty()).then(|| format!(r#"<header class="place">{}</header>"#, said.join(" ")))
}

/// A region as a section in the class of its `part`, its title the heading
/// over its document, its parts apart by the separator and its edge at the
/// heading's far end, current when the keys act on it.
fn region(reader: &mut Reader, region: &Region, part: &str, focused: bool) -> String {
    let sep = GLYPHS.spaced_sep();
    let (label, heading) = match region.title.is_empty() {
        false => {
            let title = region.title.join(&sep);
            let edge = region.edge.as_deref().map_or(String::new(), |edge| {
                format!(r#" <span class="edge">{}</span>"#, esc(edge))
            });
            (
                format!(r#" aria-label="{}""#, esc(&title)),
                format!("<h3>{}{edge}</h3>", esc(&title)),
            )
        }
        true => (String::new(), String::new()),
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
