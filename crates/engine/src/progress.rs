//! `progress.md` — one of exactly four things a node's context is built
//! from at start (a node never assumes prior history): its rendered prompt, its
//! resolved context sources, this file, and its skills. The engine
//! writes it, never an agent — regenerated in full from the log after
//! each `node_finished`, the same "state is a pure function of the log"
//! principle [`crate::replay::derive`] follows, so it never accumulates
//! narrative drift.

use yunta_core::{Node, Workflow};

use crate::replay::{derive, NodeState, RunState};

/// Renders `progress.md`'s content from `events`: every finished
/// node with its one-line description and the artifacts it produced,
/// every failed node, and everything still ahead. Pure — same workflow,
/// same log, same markdown, always.
pub fn render_progress(workflow: &Workflow, events: &[yunta_core::events::StoredEvent]) -> String {
    let state = derive(events);
    let nodes: Vec<&Node> = workflow.iter_nodes().collect();

    let mut out = String::new();
    out.push_str("# Progress\n");

    out.push_str("\n## Finished\n\n");
    render_section(&mut out, &nodes, &state, "_none yet_", |node| {
        match state.nodes.get(&node.id) {
            Some(NodeState::Finished { outcome, .. }) => {
                Some(finished_entry(&state, node, outcome))
            }
            _ => None,
        }
    });

    out.push_str("\n## Failed\n\n");
    render_section(&mut out, &nodes, &state, "_none_", |node| {
        match state.nodes.get(&node.id) {
            Some(NodeState::Failed { outcome, .. }) => {
                Some(format!("- **{}** — {}\n", node.id, fenced(outcome)))
            }
            _ => None,
        }
    });

    out.push_str("\n## Next\n\n");
    render_section(
        &mut out,
        &nodes,
        &state,
        "_nothing pending_",
        |node| match state.nodes.get(&node.id) {
            None => Some(format!("- **{}** — {}\n", node.id, description_of(node))),
            Some(NodeState::Running { .. }) => Some(format!(
                "- **{}** — {} (running)\n",
                node.id,
                description_of(node)
            )),
            _ => None,
        },
    );

    out
}

fn render_section(
    out: &mut String,
    nodes: &[&Node],
    _state: &RunState,
    empty_marker: &str,
    mut entry: impl FnMut(&Node) -> Option<String>,
) {
    let mut any = false;
    for node in nodes {
        if let Some(line) = entry(node) {
            out.push_str(&line);
            any = true;
        }
    }
    if !any {
        out.push_str(empty_marker);
        out.push('\n');
    }
}

/// A failure inside a Markdown bullet.
///
/// Two readers share this file: a person, for whom an unescaped `_` or
/// `*` silently restyles the page, and the next node's session, which
/// reads it as context. A failure that names several problems keeps
/// them, inside a fence, where neither reader has to guess where one
/// ends and the next begins.
fn fenced(outcome: &str) -> String {
    if !outcome.contains('\n') && !outcome.contains('`') {
        return format!("`{outcome}`");
    }
    format!("\n\n  ```\n{}\n  ```", indent_lines(outcome, "  "))
}

fn indent_lines(text: &str, indent: &str) -> String {
    text.lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn finished_entry(state: &RunState, node: &Node, outcome: &str) -> String {
    let mut entry = format!(
        "- **{}** — {}\n  outcome: {outcome}\n",
        node.id,
        description_of(node)
    );
    if let Some(paths) = state.artifacts.get(&node.id) {
        for path in paths {
            entry.push_str(&format!("  artifact: {}\n", path.display()));
        }
    }
    entry
}

/// A node's one-line summary for `progress.md`: its own declared
/// `description`, or its id when it declares none.
fn description_of(node: &Node) -> &str {
    node.description.as_deref().unwrap_or(node.id.as_str())
}
