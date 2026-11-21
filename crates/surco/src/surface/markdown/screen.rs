//! A screen that stays open, written as Markdown: the blocks it is made of,
//! what a terminal draws at the top of a run and at its foot, and the whole
//! screen, its parts in the order a reader takes them, said in a file's words.

use super::{block, code_span, drawn, plain_text, rows, Leaves, Page, GLYPHS};
use super::{Description, Markdown};
use crate::components::{Answer, Attention, KeyList, Legend, Noted, Region, Ribbon, Screen, Tally};
use crate::doc::Doc;
use crate::glyphs::Mark;
use crate::words::{Standing, NOTHING_NEEDS_YOU};

/// A ribbon as a line: each node's name with the word it stands at, in the
/// order the workflow declares them.
pub(super) fn ribbon(ribbon: &Ribbon) -> String {
    let sep = format!(" {} ", GLYPHS.sep());
    ribbon
        .stretches
        .iter()
        .map(|stretch| format!("{} {}", plain_text(&stretch.name), stretch.standing.word()))
        .collect::<Vec<_>>()
        .join(&sep)
}

/// A legend as a line: each key as code, beside what it does.
pub(super) fn legend(legend: &Legend) -> String {
    let sep = format!(" {} ", GLYPHS.sep());
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
    let keys: Vec<String> = list
        .keys
        .0
        .iter()
        .map(|binding| {
            let keys = code_span(binding.keys.spelled(GLYPHS));
            format!("- {keys} {}", plain_text(&binding.does))
        })
        .collect();
    list.said
        .as_deref()
        .map(|said| format!("_{}_", plain_text(said)))
        .into_iter()
        .chain((!keys.is_empty()).then(|| keys.join("\n")))
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
        .join(&format!(" {} ", GLYPHS.sep()))
}

/// A screen's keys as a line: its own, then the far keys after the separator.
fn keys(own: &Legend, anywhere: &Legend) -> String {
    [legend(own), legend(anywhere)]
        .into_iter()
        .filter(|said| !said.is_empty())
        .collect::<Vec<_>>()
        .join(&format!(" {} ", GLYPHS.sep()))
}

/// A region's title as a heading says it: its parts, then its edge, the separator between
/// two. None for a region with no title.
fn heading(region: &Region) -> Option<String> {
    let sep = format!(" {} ", GLYPHS.sep());
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
