//! Tables, runs, evidence and checklists as a browser draws them: a table
//! keeps its columns, a run's row ends with its edge, a command's output
//! quotes its end and links to the rest, and a check hangs what it means and
//! how to make it hold under its row.

use super::blocks::furrow;
use super::notices::hung;
use super::text;
use super::GLYPHS;
use crate::bars::{fine_bar, BAR_WIDTH};
use crate::components::{
    Cell, Check, Checklist, Column, Diffstat, Edge, Evidence, Found, Holds, Row, RunLine, Runs,
    Table, Whole, COLUMNS,
};
use crate::figures::signed_lines;
use crate::line::Tone;

/// A table as a browser draws it: a header of column titles when any column
/// has one, then a row for each row of the table. A table with no rows draws
/// nothing.
pub(super) fn table(table: &Table) -> String {
    if table.rows.is_empty() {
        return String::new();
    }
    let rows: String = table
        .rows
        .iter()
        .map(|row| row_html(row, &table.columns))
        .collect();
    format!(
        r#"<div class="scroll"><table>{}<tbody>{rows}</tbody></table></div>"#,
        head(&table.columns)
    )
}

/// The class a column's title and cells take, by what the column holds: a
/// state, an id, a figure and the rest of the line each read as their own
/// kind of thing. Words and prose read as plain text, so they take none.
fn holds_class(holds: Holds) -> Option<&'static str> {
    match holds {
        Holds::State => Some("state"),
        Holds::Id => Some("id"),
        Holds::Figure => Some("figure"),
        Holds::Rest => Some("rest"),
        Holds::Words | Holds::Prose => None,
    }
}

/// `<name class="CLASS">inner</name>`, or `<name>inner</name>` when there is
/// no class to give.
fn element(name: &str, class: Option<&str>, inner: &str) -> String {
    match class {
        Some(class) => format!(r#"<{name} class="{class}">{inner}</{name}>"#),
        None => format!("<{name}>{inner}</{name}>"),
    }
}

/// `text`, already escaped, in the tone it is read in: its class's span when
/// the tone has one, bold when it is what a line is about, a copyable code
/// span when it is a command, and as it stands otherwise.
fn toned(tone: Tone, text: &str) -> String {
    match (text::class_of(tone), tone) {
        (Some(class), _) => format!(r#"<span class="{class}">{text}</span>"#),
        (None, Tone::Strong) => format!("<b>{text}</b>"),
        (None, Tone::Command) => format!(r#"<code class="command">{text}</code>"#),
        (None, _) => text.to_string(),
    }
}

/// The header of a table: each column's title under its class. A table whose
/// columns have no titles has no header at all.
fn head(columns: &[Column]) -> String {
    if columns.iter().all(|column| column.title.is_empty()) {
        return String::new();
    }
    let titles: String = columns
        .iter()
        .map(|column| element("th", holds_class(column.holds), &text::esc(column.title)))
        .collect();
    format!("<thead><tr>{titles}</tr></thead>")
}

/// One row of a table: each cell under its column, and the row's mark in
/// front of its first cell.
fn row_html(row: &Row, columns: &[Column]) -> String {
    let cells: String = row
        .cells
        .iter()
        .zip(columns)
        .enumerate()
        .map(|(at, (cell, column))| {
            let text = cell_html(cell, column.holds);
            let text = match (at, row.mark) {
                (0, Some(mark)) => format!("{} {text}", text::mark(mark)),
                _ => text,
            };
            element("td", holds_class(column.holds), &text)
        })
        .collect();
    format!("<tr>{cells}</tr>")
}

/// One cell's text as a browser shows it: an id is escaped as it stands, since
/// an id is not prose, and any other text is words; then it is wrapped in its
/// tone.
fn cell_html(cell: &Cell, holds: Holds) -> String {
    let text = match holds {
        Holds::Id => text::esc(&cell.text),
        Holds::State | Holds::Words | Holds::Figure | Holds::Rest | Holds::Prose => {
            text::words(&cell.text)
        }
    };
    toned(cell.tone, &text)
}

/// Runs as a table without a header: each run's row says where it stands,
/// its handle, what it runs and in which mode, the furrow its nodes plow,
/// what holds it, and its edge. A list with no runs draws nothing.
pub(super) fn runs(runs: &Runs) -> String {
    if runs.rows.is_empty() {
        return String::new();
    }
    let rows: String = runs.rows.iter().map(run_row).collect();
    format!(r#"<div class="scroll"><table><tbody>{rows}</tbody></table></div>"#)
}

/// One run as a table row, its cells in the order a reader answers them.
fn run_row(run: &RunLine) -> String {
    let mark = run.standing.mark();
    let state = format!(
        "{} {}",
        text::mark(mark),
        toned(Tone::of(mark), run.standing.word())
    );
    let cells = [
        element("td", Some("state"), &state),
        element("td", Some("id"), &text::esc(&run.handle)),
        element(
            "td",
            None,
            &format!(
                "{} ({})",
                text::words(&run.workflow),
                text::words(&run.mode)
            ),
        ),
        element("td", None, &furrow(&run.furrow)),
        element("td", Some("rest"), &text::words(&run.said)),
        element("td", Some("figure"), &edge(&run.edge)),
    ];
    format!("<tr>{}</tr>", cells.concat())
}

/// A run's edge: how long it has stood where it is, as words, or the button
/// that copies the command moving it on.
fn edge(edge: &Edge) -> String {
    match edge {
        Edge::Age(age) => text::words(age),
        Edge::Command(command) => text::command(command),
    }
}

/// What a command printed: its last lines quoted, how many lines are above
/// them, and where the whole output is. Evidence with nothing to quote and no
/// whole output draws nothing.
pub(super) fn evidence(evidence: &Evidence) -> String {
    let quoted = evidence.quoted();
    if quoted.is_empty() && evidence.whole.is_none() {
        return String::new();
    }
    let lines: Vec<String> = quoted.iter().map(|line| text::esc(line)).collect();
    let mut out = format!(r#"<div class="evidence"><pre>{}</pre>"#, lines.join("\n"));
    let above = evidence.above();
    if above > 0 {
        out.push_str(&format!(
            r#"<p class="above">{} above</p>"#,
            crate::text::counted(above, "line")
        ));
    }
    if let Some(whole) = &evidence.whole {
        out.push_str(&whole_output(whole));
    }
    out.push_str("</div>");
    out
}

/// Where the whole of a command's output is: a button that copies the command
/// showing it, or a link to the file it is kept in, shown as a path.
fn whole_output(whole: &Whole) -> String {
    match whole {
        Whole::Command(command) => format!(
            r#"<div class="steps">{} <span class="does">whole output</span></div>"#,
            text::command(command)
        ),
        Whole::File { shown, path } => format!(
            r#"<p class="above">whole output: <a href="{}"><code>{}</code></a></p>"#,
            text::esc(&crate::line::file_url(path)),
            text::esc(shown)
        ),
    }
}

/// What a set of checks found: the checks that hold folded into one row when
/// the list folds them, then a row for each check that has its own. A list
/// with no rows draws nothing.
pub(super) fn checklist(list: &Checklist) -> String {
    let folded = list
        .fold()
        .map(|fold| fold_row(&fold.subject, &fold.names))
        .unwrap_or_default();
    let rows: String = list.unfolded().map(check_row).collect();
    if folded.is_empty() && rows.is_empty() {
        return String::new();
    }
    format!(r#"<ul class="checks">{folded}{rows}</ul>"#)
}

/// The row the checks that hold fold into: how many there are, and the names
/// of each, a separator apart.
fn fold_row(subject: &str, names: &[&str]) -> String {
    let names: Vec<String> = names.iter().map(|name| text::words(name)).collect();
    let separator = format!(" {} ", GLYPHS.sep());
    format!(
        r#"<li>{} <span class="subject">{}</span> <span>{}</span></li>"#,
        text::mark(Found::Holds.mark()),
        text::words(&crate::text::one_line(subject)),
        names.join(separator.as_str()),
    )
}

/// One check with a row of its own: its mark, its subject and what it found,
/// then what it means and how to make it hold, and each line of detail it has
/// that is not blank.
fn check_row(check: &Check) -> String {
    let details: String = check
        .details
        .iter()
        .flat_map(|detail| detail.lines())
        .filter(|line| !line.trim().is_empty())
        .map(|line| format!("<li>{}</li>", text::words(line)))
        .collect();
    let details = match details.is_empty() {
        true => String::new(),
        false => format!(r#"<ul class="details">{details}</ul>"#),
    };
    format!(
        r#"<li>{} <span class="subject">{}</span> <span>{}</span>{}{details}</li>"#,
        text::mark(check.found.mark()),
        text::words(&crate::text::one_line(&check.subject)),
        text::words(&check.said),
        hung(check.so.as_deref(), check.fix.as_ref()),
    )
}

/// A change, file by file: each file's path as code, its figures in their tones,
/// and a bar of each against the file that moves most; then what the rest come to.
pub(super) fn diffstat(stat: &Diffstat) -> String {
    if stat.files.is_empty() {
        return String::new();
    }
    let [path, added, removed] = COLUMNS;
    let column = |title, holds| Column { title, holds };
    let titles = head(&[
        column(path, Holds::Id),
        column(added, Holds::Figure),
        column(removed, Holds::Figure),
        column("", Holds::Words),
    ]);
    let figure = |said: String, class| match said.is_empty() {
        true => r#"<td class="figure"></td>"#.to_string(),
        false => format!(r#"<td class="figure"><span class="{class}">{said}</span></td>"#),
    };
    let scale = stat.scale() as f64;
    let bar = |count: u64| fine_bar(count as f64, scale, BAR_WIDTH, GLYPHS).drawn();
    let rows: String = stat
        .files
        .iter()
        .map(|file| {
            let [added, removed] = signed_lines(file.added, file.removed, GLYPHS);
            format!(
                r#"<tr><td class="id"><code>{}</code></td>{}{}<td class="bar"><span class="done">{}</span><span class="failed">{}</span></td></tr>"#,
                text::esc(&file.path),
                figure(added, "done"),
                figure(removed, "failed"),
                bar(file.added),
                bar(file.removed),
            )
        })
        .collect();
    let more = stat
        .more
        .as_ref()
        .map(|more| format!(r#"<p class="foot">{}</p>"#, text::esc(&more.said(GLYPHS))))
        .unwrap_or_default();
    format!(
        r#"<div class="scroll diffstat"><table>{titles}<tbody>{rows}</tbody></table>{more}</div>"#
    )
}
