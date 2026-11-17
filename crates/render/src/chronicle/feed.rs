//! A node's feed: what happens at a node, a row a moment, in the chronicle's own words.
//!
//! One more surface of the same words: a row says what [`line`](super::line) says of its
//! moment, without the subject the pane already names and without its mark. A call's verb
//! stands in a column as wide as the widest word a call is read by, so every object starts at
//! one column whichever verbs stand in sight; where calls of more than one task stand in
//! sight, each names its task after its verb. Which moments a feed lists is [`in_feed`]'s
//! alone; which of them stand in sight, and how many above and under, is its caller's.

use std::collections::{BTreeMap, BTreeSet};

use surco::components::Feed;
use surco::glyphs::Glyphs;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::text::counted;
use surco::width::cell_width;
use yunta_core::events::{artifacts, gates, node, scope, session, NodeState};
use yunta_core::{SessionId, TaskId};
use yunta_replay::frame::{Happening, Moment};

/// The cells a call's verb takes in a feed: the widest word a call is read by, `search` and
/// `submit`, and two that part it from its object, so every object stands at one column.
const VERB: usize = 8;

/// The cells that part a call's task from its object.
const APART: usize = 2;

/// Whether a node's feed lists this moment: each call of its sessions, where each attempt
/// opens and where it ends, and what the engine does there between the calls (a session
/// opened, a write refused, a file formatted, a task's check and its result, an artifact
/// handed over or refused, a finding, a scope asked and answered). What a session reports
/// spending, its notes, the runner and the context it was given, the rail's summary, a scope
/// check's count and a task's moves stay out: a pane says them in its own parts, or no reader
/// of a node's work acts on them.
pub fn in_feed(happening: &Happening) -> bool {
    use node::happening::Happening as Node;
    use session::happening::Happening as Session;
    let listed = match happening {
        Happening::Session(it) => matches!(
            it,
            Session::Opened { .. }
                | Session::Called { .. }
                | Session::Refused(_)
                | Session::Formatted { .. }
        ),
        Happening::Node(it) => matches!(
            it,
            Node::Reached {
                state: NodeState::Running { .. }
                    | NodeState::Finished { .. }
                    | NodeState::Failed { .. },
                ..
            } | Node::CriteriaChecked { .. }
        ),
        Happening::Scope(it) => matches!(it, scope::happening::Happening::Expansion { .. }),
        Happening::Findings(_) => true,
        Happening::Artifacts(it) => matches!(
            it,
            artifacts::happening::Happening::Accepted(_)
                | artifacts::happening::Happening::Submitted { taken: false, .. }
        ),
        Happening::Gates(it) => matches!(it, gates::happening::Happening::Resolved(_)),
        _ => false,
    };
    listed && super::says_anything(happening)
}

/// A node's feed with `moments` in sight, each a moment [`in_feed`] lists, the oldest first,
/// and `before` rows above them and `after` under. A call is its verb in its column, then its
/// object; where calls of more than one task stand in sight, its task after its verb, from
/// `tasks`, the task each session worked. Any other moment is the words its line says after
/// its subject. The newest row is strong where it stands in sight.
pub fn feed(
    moments: &[Moment],
    tasks: &BTreeMap<SessionId, TaskId>,
    before: usize,
    after: usize,
    look: &Look,
) -> Feed {
    let task_of = |moment: &Moment| match &moment.happening {
        Happening::Session(session::happening::Happening::Called {
            session: Some(session),
            ..
        }) => tasks.get(session),
        _ => None,
    };
    let named: BTreeSet<&TaskId> = moments.iter().filter_map(task_of).collect();
    let column = match named.len() > 1 {
        true => {
            named
                .iter()
                .map(|task| cell_width(task.as_str()))
                .max()
                .unwrap_or(0)
                + APART
        }
        false => 0,
    };
    let newest = moments.len().checked_sub(1).filter(|_| after == 0);
    let rows = moments
        .iter()
        .enumerate()
        .map(|(at, moment)| {
            let row = row(moment, task_of(moment), column, look.glyphs);
            match Some(at) == newest {
                true => Line::new().push(Tone::Strong, row),
                false => Line::new().plain(row),
            }
        })
        .collect();
    Feed {
        rows,
        before,
        after,
    }
}

/// What a feed stands under: what the node is doing, and how many calls its open attempt
/// made.
pub fn feed_heading(calls: usize, glyphs: Glyphs) -> String {
    format!(
        "what it is doing {} {}",
        glyphs.sep(),
        counted(calls, "call")
    )
}

/// One moment's row: a call's verb in its column, then its task in a `column` as wide as the
/// widest task in sight where one is named, then its object; any other moment the words that
/// follow its subject.
fn row(moment: &Moment, task: Option<&TaskId>, column: usize, glyphs: Glyphs) -> String {
    let words = super::phrased(moment, glyphs).words;
    let Happening::Session(session::happening::Happening::Called { action, .. }) =
        &moment.happening
    else {
        return words;
    };
    let verb = action.as_str();
    let object = words.strip_prefix(verb).unwrap_or(&words).trim_start();
    let task = match column {
        0 => String::new(),
        _ => format!("{:<column$}", task.map_or("", TaskId::as_str)),
    };
    format!("{verb:<VERB$}{task}{object}")
        .trim_end()
        .to_string()
}
