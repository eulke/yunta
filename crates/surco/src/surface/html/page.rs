//! A document as one page a browser reads: the file's skeleton, its title, and
//! its body in three columns: what the page holds, the document, and the
//! decision the run waits on.

use super::blocks::{self, Reader};
use super::style;
use super::text::esc;
use super::GLYPHS;
use crate::doc::{Block, Doc};

/// The whole file: a head with its title, tokens and style, the body, and the
/// script that copies a command.
pub(super) fn whole(doc: &Doc) -> String {
    let title = title(doc);
    let tokens = style::tokens();
    let css = include_str!("page.css");
    let body = body(doc);
    let script = include_str!("copy.js");
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'">
<title>{title}</title>
<style>
{tokens}
{css}
</style>
</head>
<body>
{body}
<script>
{script}
</script>
</body>
</html>
"#
    )
}

/// The title of the page, escaped: the subject, the word and the handle of the
/// first headline, a dot apart; else the name of the first title; else `yunta`.
fn title(doc: &Doc) -> String {
    let headline = doc.blocks().iter().find_map(|block| match block {
        Block::Headline(headline) => Some(headline),
        _ => None,
    });
    let name = doc.blocks().iter().find_map(|block| match block {
        Block::Title(title) => Some(title.name.text()),
        _ => None,
    });
    let said = match (headline, name) {
        (Some(headline), _) => {
            let sep = format!(" {} ", GLYPHS.sep());
            std::iter::once(headline.subject.clone())
                .chain(std::iter::once(headline.said.clone()))
                .chain(headline.edge.clone())
                .collect::<Vec<_>>()
                .join(sep.as_str())
        }
        (None, Some(name)) => name,
        (None, None) => "yunta".to_string(),
    };
    esc(&said)
}

/// The body of the page, what a golden holds: the list of what it holds when it
/// holds at least two things to link, the document's blocks, and the decision
/// in the column beside them, each block on a line of its own.
pub(super) fn body(doc: &Doc) -> String {
    let decision = doc.decision_at();
    let mut main = Reader::new();
    let main_blocks = doc
        .blocks()
        .iter()
        .enumerate()
        .filter(|(at, _)| Some(*at) != decision)
        .map(|(_, block)| blocks::block(&mut main, block, 0))
        .filter(|drawn| !drawn.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let nav = contents(&main);
    let aside = decision
        .map(|at| {
            // The decision's ids go on from the document's, so no two repeat.
            let mut reader = Reader::after(&main);
            blocks::block(&mut reader, &doc.blocks()[at], 0)
        })
        .filter(|drawn| !drawn.trim().is_empty());
    let mut classes = vec!["page"];
    if nav.is_none() {
        classes.push("no-toc");
    }
    if aside.is_none() {
        classes.push("no-decide");
    }
    let mut lines = vec![format!(r#"<div class="{}">"#, classes.join(" "))];
    lines.extend(nav);
    lines.push(r#"<main class="doc">"#.to_string());
    lines.push(main_blocks);
    lines.push("</main>".to_string());
    if let Some(aside) = aside {
        lines.push(r#"<aside class="decide" aria-label="what this run waits on">"#.to_string());
        lines.push(aside);
        lines.push("</aside>".to_string());
    }
    lines.push("</div>".to_string());
    lines.join("\n")
}

/// The list of what the page holds: a link to each section, heading or fold it
/// draws one level down at most, a level in for the nested ones. `None` when
/// fewer than two links would be listed, since one link is no list.
fn contents(reader: &Reader) -> Option<String> {
    let links: Vec<String> = reader
        .toc
        .iter()
        .filter(|entry| entry.depth <= 1)
        .map(|entry| {
            let class = match entry.depth {
                0 => "",
                _ => r#" class="sub""#,
            };
            format!(
                r##"<a{class} href="#{}">{}</a>"##,
                esc(&entry.id),
                entry.title
            )
        })
        .collect();
    (links.len() >= 2).then(|| {
        format!(
            r#"<nav class="toc" aria-label="on this page"><h2>on this page</h2>{}</nav>"#,
            links.concat()
        )
    })
}
