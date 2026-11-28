//! A node's feed: what happens at a node, a row a moment, in the chronicle's own words.
//!
//! One more surface of the same words: a row says what [`line`](super::line) says of its
//! moment, without the subject the pane already names and without its mark. A call's verb
//! stands in a column as wide as the widest word a call is read by, so every object starts at
//! one column whichever verbs stand in sight; where calls of more than one task stand in
//! sight, each names its task after its verb. Which moments a feed lists is [`in_feed`]'s
//! alone; which of them stand in sight, and how many above and under, is its caller's.
//!
//! The run's whole chronicle is a feed of its own ([`whole`]): every moment that says anything,
//! each row about its subject, which the surface sets in a column of its own.

use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};

use surco::components::{Feed, FeedRow};
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
/// handed over or refused, a finding, a scope asked and answered, a decision answered, the
/// questions it asks a person and their answers). What a session reports spending, its notes, the runner and
/// the context it was given, the rail's summary, a scope check's count and a task's moves
/// stay out: a pane says them in its own parts, or no reader of a node's work acts on them.
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
        Happening::Gates(it) => matches!(
            it,
            gates::happening::Happening::Resolved(_)
                | gates::happening::Happening::Asked { .. }
                | gates::happening::Happening::Answered { .. }
        ),
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
            FeedRow::of(match Some(at) == newest {
                true => Line::new().push(Tone::Strong, row),
                false => Line::new().plain(row),
            })
        })
        .collect();
    Feed {
        rows,
        before,
        after,
    }
}

/// The run's whole chronicle as a feed of its own, with `moments` in sight, each a moment that
/// says anything ([`says_anything`](super::says_anything)), the oldest first, and `before`
/// rows above them and `after` under: each row is about its moment's subject, the node or the
/// run, which a surface says before it, and says what its line says after that subject as a
/// node's feed says it ([`feed`]), every moment listed. The newest row is strong where it
/// stands in sight. Not [`feed`], one node's, whose pane names the node.
pub fn whole(
    moments: &[Moment],
    tasks: &BTreeMap<SessionId, TaskId>,
    before: usize,
    after: usize,
    look: &Look,
) -> Feed {
    let fed = feed(moments, tasks, before, after, look);
    let rows = moments
        .iter()
        .zip(fed.rows)
        .map(|(moment, row)| FeedRow::about(super::phrased(moment, look.glyphs).subject, row.line))
        .collect();
    Feed { rows, ..fed }
}

/// What the run's whole chronicle stands under: how many moments it holds.
pub fn whole_heading(moments: usize, glyphs: Glyphs) -> String {
    format!(
        "its whole chronicle {} {}",
        glyphs.sep(),
        counted(moments, "moment")
    )
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

/// How many calls the open attempt of a node whose moments are `moments`, in the log's order,
/// made: those after the moment its latest attempt started, open or closed since. What a
/// feed's heading counts, wherever a node's feed is said.
pub fn open_attempt_calls<M: Borrow<Moment>>(moments: &[M]) -> usize {
    let mut calls = OpenAttemptCalls::default();
    moments
        .iter()
        .for_each(|moment| calls.took(moment.borrow()));
    calls.count()
}

/// The calls a node's open attempt made, counted one moment at a time in the log's order, as
/// [`open_attempt_calls`] counts them over a node's moments whole: for a reader that keeps the
/// count as the moments come, so that what a feed's heading says walks no call again. Not the
/// calls of every attempt, which a node's feed lists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenAttemptCalls(usize);

impl OpenAttemptCalls {
    /// `moment` counted: the moment an attempt starts counts from none, and a call one more.
    pub fn took(&mut self, moment: &Moment) {
        match &moment.happening {
            Happening::Node(node::happening::Happening::Reached {
                state: NodeState::Running { .. },
                ..
            }) => self.0 = 0,
            Happening::Session(session::happening::Happening::Called { .. }) => self.0 += 1,
            _ => {}
        }
    }

    /// How many calls the open attempt made, by the moments counted so far.
    pub fn count(self) -> usize {
        self.0
    }
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
