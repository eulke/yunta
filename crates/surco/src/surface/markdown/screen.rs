//! A screen that stays open, written as Markdown: the blocks it is made of,
//! what a terminal draws at the top of a run and at its foot, and the whole
//! screen, its parts in the order a reader takes them, said in a file's words.

use super::{block, code_span, drawn, plain_text, rows, Leaves, Page, GLYPHS};
use super::{Description, Markdown};
use crate::components::{
    Answer, AtAGlance, Attention, Binding, KeyList, Legend, Noted, Region, Ribbon, SameAtAShell,
    Screen, Stretch, Tally, WordChoice,
};
use crate::doc::Doc;
use crate::glyphs::Mark;
use crate::words::{Standing, NOTHING_NEEDS_YOU};

/// The commands that do the same at a shell, a command an item, each as code on one line.
pub(super) fn same_at_a_shell(same: &SameAtAShell) -> String {
    same.0
        .iter()
        .map(|command| format!("- {}", code_span(&crate::text::one_line(command))))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A row of words, apart by the separator, the chosen one named after the pointer, in bold.
pub(super) fn word_choice(choice: &WordChoice) -> String {
    let chosen = choice.chosen_word();
    choice
        .words
        .iter()
        .enumerate()
        .map(|(at, word)| match at == chosen {
            true => format!("{} **{}**", GLYPHS.pointer(), plain_text(word)),
            false => plain_text(word),
        })
        .collect::<Vec<_>>()
        .join(&GLYPHS.spaced_sep())
}

/// A ribbon as a line: each node's name with the word it stands at, in the
/// order the workflow declares them.
pub(super) fn ribbon(ribbon: &Ribbon) -> String {
    stretches(&ribbon.stretches)
}

/// Steps as a line, each by its name and the word it stands at, as a ribbon and a form's foot
/// are written: what their strokes draw at a terminal.
pub(super) fn stretches(stretches: &[Stretch]) -> String {
    let sep = GLYPHS.spaced_sep();
    stretches
        .iter()
        .map(|stretch| format!("{} {}", plain_text(&stretch.name), stretch.standing.word()))
        .collect::<Vec<_>>()
        .join(&sep)
}

/// A legend as a line: each key as code, beside what it does.
pub(super) fn legend(legend: &Legend) -> String {
    let sep = GLYPHS.spaced_sep();
    legend
        .0
        .iter()
        .map(|binding| {
            let keys = code_span(binding.keys.spelled(GLYPHS));
            format!("{keys} {}", plain_text(&binding.does))
        })
        .collect::<Vec<_>>()
        .join(&sep)
}

/// A list of keys: its sentence, then a key an item, the key as code beside what it does.
pub(super) fn key_list(list: &KeyList) -> String {
    keys_under(list.said.as_deref(), &list.keys.0)
}

/// `keys` under `said`, in italics where there is one: a key an item, the key as code beside
/// what it does. Every list of keys a file writes says its keys so.
fn keys_under(said: Option<&str>, keys: &[Binding]) -> String {
    let keys: Vec<String> = keys
        .iter()
        .map(|binding| {
            let keys = code_span(binding.keys.spelled(GLYPHS));
            format!("- {keys} {}", plain_text(&binding.does))
        })
        .collect();
    said.map(|said| format!("_{}_", plain_text(said)))
        .into_iter()
        .chain((!keys.is_empty()).then(|| keys.join("\n")))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A run at a glance: each fact that says something an item, its label before its words;
/// then the acts as a list of keys under their heading, where the glance offers any.
pub(super) fn at_a_glance(glance: &AtAGlance) -> String {
    let facts: Vec<String> = glance
        .said()
        .map(|(label, value)| format!("- {}: {}", plain_text(label), plain_text(value)))
        .collect();
    let acts = glance
        .acts
        .as_ref()
        .filter(|(_, acts)| !acts.is_empty())
        .map(|(heading, acts)| keys_under(Some(heading), acts));
    (!facts.is_empty())
        .then(|| facts.join("\n"))
        .into_iter()
        .chain(acts)
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// An attention as a line: the mark and `needs you`, its subject in bold,
/// and the answer where the terminal says it: a command as code before what
/// is asked, a key as code after it, and what is asked alone when it is
/// answered at another terminal or said where a line is typed.
pub(super) fn attention(attention: &Attention) -> String {
    if attention.asks.trim().is_empty() {
        return String::new();
    }
    let mark = GLYPHS.mark(Mark::NeedsYou);
    let word = Standing::NeedsYou.word();
    let head = match &attention.subject {
        None => format!("{mark} {word}: "),
        Some(subject) => format!("{mark} **{}** {word}: ", plain_text(subject)),
    };
    let asks = plain_text(&attention.asks);
    match &attention.answer {
        Answer::Command(command) => format!("{head}{} ({asks})", code_span(command)),
        Answer::Elsewhere | Answer::Unkeyed => format!("{head}{asks}"),
        Answer::Key(binding) => format!(
            "{head}{asks} {} {} {}",
            GLYPHS.sep(),
            code_span(binding.keys.spelled(GLYPHS)),
            plain_text(&binding.does)
        ),
    }
}

/// A note as a line: its mark, then what the act did.
pub(super) fn noted(noted: &Noted) -> String {
    rows(&[noted.said(GLYPHS)])
}

impl Markdown {
    /// A screen as a Markdown file: where the person is and what is
    /// counted, what it is about, the side region then the main one under
    /// their titles, each with its edge, what the run it is about has spent,
    /// what needs the person, or in its place what their last act came to, and
    /// the keys, the far keys after the screen's own, then what is open over it,
    /// under its title.
    pub fn screen(&self, screen: &Screen) -> String {
        written(
            screen,
            Page {
                description: false,
                leaves: Leaves::Nothing,
                folded: false,
            },
        )
    }
}

impl Description {
    /// A screen as a forge's page: as a file writes it, but that its first
    /// headline is a line under the title the forge draws.
    pub fn screen(&self, screen: &Screen) -> String {
        written(
            screen,
            Page {
                description: true,
                leaves: self.leaves,
                folded: false,
            },
        )
    }
}

/// `screen` written as `page` says, its parts separated by a blank line.
fn written(screen: &Screen, page: Page) -> String {
    let mut parts = vec![place(screen)];
    parts.extend(screen.about.iter().map(|about| drawn(about, page)));
    if let Some(side) = &screen.side {
        parts.push(region(heading(&side.region), &side.region.doc, page));
    }
    parts.push(region(heading(&screen.main), &screen.main.doc, page));
    parts.extend(screen.meter.iter().map(|meter| rows(&[meter.said(GLYPHS)])));
    parts.extend(match &screen.noted {
        Some(said) => Some(noted(said)),
        None => screen.attention.as_ref().map(attention),
    });
    parts.push(keys(&screen.legend, &screen.anywhere));
    if let Some(overlay) = &screen.overlay {
        parts.push(region(Some(plain_text(&overlay.title)), &overlay.doc, page));
        parts.push(legend(&overlay.legend));
    }
    let said: Vec<&str> = parts
        .iter()
        .map(|part| part.trim_end())
        .filter(|part| !part.is_empty())
        .collect();
    match said.is_empty() {
        true => String::new(),
        false => format!("{}\n", said.join("\n\n")),
    }
}

/// Where the person is, the outermost place in bold, then how many runs
/// stand at each word that asks for a look, each count after its mark, or that
/// nothing needs the person where the runs counted need nothing.
fn place(screen: &Screen) -> String {
    let within = format!(" {} ", GLYPHS.within());
    let trail: Vec<String> = screen
        .trail
        .iter()
        .enumerate()
        .map(|(at, place)| match at {
            0 => format!("**{}**", plain_text(place)),
            _ => plain_text(place),
        })
        .collect();
    let counts: Vec<String> = match &screen.tally {
        Tally::Uncounted => Vec::new(),
        Tally::Counted(counts) if counts.is_empty() => vec![NOTHING_NEEDS_YOU.to_string()],
        Tally::Counted(counts) => counts
            .iter()
            .map(|count| {
                format!(
                    "{} {}",
                    GLYPHS.mark(count.standing.mark()),
                    plain_text(&count.said())
                )
            })
            .collect(),
    };
    (!trail.is_empty())
        .then(|| trail.join(&within))
        .into_iter()
        .chain(counts)
        .collect::<Vec<_>>()
        .join(&GLYPHS.spaced_sep())
}

/// A screen's keys as a line: its own, then the far keys after the separator.
fn keys(own: &Legend, anywhere: &Legend) -> String {
    [legend(own), legend(anywhere)]
        .into_iter()
        .filter(|said| !said.is_empty())
        .collect::<Vec<_>>()
        .join(&GLYPHS.spaced_sep())
}

/// A region's title as a heading says it: its parts, then its edge, the separator between
/// two. None for a region with no title.
fn heading(region: &Region) -> Option<String> {
    let sep = GLYPHS.spaced_sep();
    (!region.title.is_empty()).then(|| {
        region
            .title
            .iter()
            .chain(&region.edge)
            .map(|part| plain_text(part))
            .collect::<Vec<_>>()
            .join(&sep)
    })
}

/// A region as a section: its heading, of the second level, when it has one, and each block
/// of its document under it.
fn region(heading: Option<String>, doc: &Doc, page: Page) -> String {
    let depth = usize::from(heading.is_some());
    heading
        .map(|heading| format!("## {heading}"))
        .into_iter()
        .chain(doc.blocks().iter().map(|inner| block(inner, depth, page)))
        .filter(|drawn| !drawn.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}
