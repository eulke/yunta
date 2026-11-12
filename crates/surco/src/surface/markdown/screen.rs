//! A screen that stays open, written as Markdown: the blocks it is made of,
//! what a terminal draws at the top of a run and at its foot, and the whole
//! screen, its parts in the order a reader takes them, said in a file's words.

use super::{block, code_span, drawn, plain_text, Leaves, Page, GLYPHS};
use super::{Description, Markdown};
use crate::components::{Answer, Attention, Legend, Ribbon, Screen};
use crate::doc::Doc;
use crate::glyphs::Mark;
use crate::words::Standing;

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

/// An attention as a line: the mark and `needs you`, its subject in bold,
/// and the answer where the terminal says it: a command as code before what
/// is asked, a key as code after it, and what is asked alone when it is
/// answered at another terminal.
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
        Answer::Elsewhere => format!("{head}{asks}"),
        Answer::Key(binding) => format!(
            "{head}{asks} {} {} {}",
            GLYPHS.sep(),
            code_span(binding.keys.spelled(GLYPHS)),
            plain_text(&binding.does)
        ),
    }
}

impl Markdown {
    /// A screen as a Markdown file: where the person is and what is
    /// counted, what it is about, the side region then the main one under
    /// their titles, what needs the person and the keys, then what is open
    /// over it, under its title.
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
        parts.push(region(side.region.title.as_deref(), &side.region.doc, page));
    }
    parts.push(region(screen.main.title.as_deref(), &screen.main.doc, page));
    parts.extend(screen.attention.iter().map(attention));
    parts.push(legend(&screen.legend));
    if let Some(overlay) = &screen.overlay {
        parts.push(region(Some(&overlay.title), &overlay.doc, page));
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
/// stand at each word that asks for a look, each count after its mark.
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
    let counts = screen.tally.iter().map(|count| {
        format!(
            "{} {}",
            GLYPHS.mark(count.standing.mark()),
            plain_text(&count.said())
        )
    });
    (!trail.is_empty())
        .then(|| trail.join(&within))
        .into_iter()
        .chain(counts)
        .collect::<Vec<_>>()
        .join(&format!(" {} ", GLYPHS.sep()))
}

/// A region as a section: its title a heading of the second level when it
/// has one, and each block of its document under it.
fn region(title: Option<&str>, doc: &Doc, page: Page) -> String {
    let depth = usize::from(title.is_some());
    title
        .map(|title| format!("## {}", plain_text(title)))
        .into_iter()
        .chain(doc.blocks().iter().map(|inner| block(inner, depth, page)))
        .filter(|drawn| !drawn.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}
