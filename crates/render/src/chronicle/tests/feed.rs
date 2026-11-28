//! A node's feed: which of a node's moments it lists, and each one's row, a call's verb in a
//! column of its own and the newest row strong.

use std::collections::BTreeMap;
use std::path::Path;

use surco::doc::Block;
use surco::glyphs::Glyphs;
use surco::line::Tone;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_core::events::scope::happening::Step;
use yunta_core::events::{
    artifacts, gates, node, scope, session, tasks, AgentMessageType, ArtifactId, Channel,
    NodeState, Phase, TaskStatus, ToolAction, ToolTarget,
};
use yunta_core::{ArtifactKind, QuestionId, SessionId, TaskId};
use yunta_replay::frame::{Happening, Moment};

use super::super::{feed, feed_heading, in_feed, whole, whole_heading};

fn look(glyphs: Glyphs) -> Look {
    Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::of(Some(120), None),
        motion: Motion::Moves,
    }
}

fn at_the_node(happening: Happening) -> Moment {
    Moment::sample(Some("implement"), happening)
}

/// A call of `action` on `object`, made in `session` where it names one.
fn call(action: ToolAction, object: &str, session: Option<&str>) -> Moment {
    at_the_node(Happening::Session(session::happening::Happening::Called {
        tool: None,
        action,
        target: Some(ToolTarget::of_path(Path::new(object))),
        session: session.map(SessionId::from),
    }))
}

fn reached(state: NodeState) -> Moment {
    at_the_node(Happening::Node(node::happening::Happening::Reached {
        state,
        elapsed: None,
        children: Vec::new(),
    }))
}

fn opened() -> Moment {
    at_the_node(Happening::Session(session::happening::Happening::Opened {
        agent: None,
        model: Some("gpt-6-luna".into()),
        fence: None,
    }))
}

fn pre_check(task: &str) -> Moment {
    at_the_node(Happening::Node(
        node::happening::Happening::CriteriaChecked {
            task: task.into(),
            phase: Phase::Pre,
            checked: 1,
            red: Vec::new(),
        },
    ))
}

fn texts(moments: &[Moment], tasks: &BTreeMap<SessionId, TaskId>) -> Vec<String> {
    feed(moments, tasks, 0, 0, &look(Glyphs::Unicode))
        .rows
        .iter()
        .map(|row| row.line.text())
        .collect()
}

/// A call is its verb in a column eight cells wide, then its object; an attempt's opening and
/// what the engine does say the words that follow their subject; and the newest row is strong
/// only while it stands in sight.
#[test]
fn a_call_stands_in_its_verb_column_and_any_other_moment_says_its_words() {
    let moments = [
        reached(NodeState::Running { attempt: 1 }),
        opened(),
        call(ToolAction::Read, "crates/cli/src/pack.rs", None),
        call(ToolAction::Submit, "tasks", None),
    ];
    assert_eq!(
        texts(&moments, &BTreeMap::new()),
        [
            "running: attempt 1",
            "session opened on gpt-6-luna",
            "read    crates/cli/src/pack.rs",
            "submit  tasks",
        ]
    );
    let strong = |after: usize| -> Vec<bool> {
        feed(&moments, &BTreeMap::new(), 2, after, &look(Glyphs::Unicode))
            .rows
            .iter()
            .map(|row| {
                row.line
                    .spans()
                    .iter()
                    .all(|span| span.tone == Tone::Strong)
            })
            .collect()
    };
    assert_eq!(strong(0), [false, false, false, true]);
    assert_eq!(strong(3), [false; 4]);
}

/// Where calls of two tasks stand in sight, each names its task after its verb, in a column as
/// wide as the widest task and two cells; with one task's calls in sight, none names it.
#[test]
fn calls_of_two_tasks_in_sight_each_name_their_task_after_the_verb() {
    let tasks: BTreeMap<SessionId, TaskId> = [("s-1", "task-1"), ("s-2", "task-2")]
        .into_iter()
        .map(|(session, task)| (session.into(), task.into()))
        .collect();
    let moments = [
        call(ToolAction::Read, "README.md", Some("s-1")),
        pre_check("task-2"),
        call(ToolAction::Read, "README.md", Some("s-2")),
        call(ToolAction::Write, "greeting.txt", Some("s-1")),
        call(ToolAction::Write, "farewell.txt", Some("s-2")),
    ];
    assert_eq!(
        texts(&moments, &tasks),
        [
            "read    task-1  README.md",
            "task-2 pre: 1 criterion",
            "read    task-2  README.md",
            "write   task-1  greeting.txt",
            "write   task-2  farewell.txt",
        ]
    );
    assert_eq!(
        texts(&moments[3..4], &tasks),
        ["write   greeting.txt"],
        "one task's calls alone name none"
    );
}

/// Every word a call is read by leaves its object at the column the targets draw it at.
#[test]
fn every_call_s_object_stands_at_one_column_whatever_its_verb() {
    // The match names every action, so a new one stops this test's build until it is listed
    // below too.
    let listed = |action: ToolAction| match action {
        ToolAction::Read
        | ToolAction::Edit
        | ToolAction::Write
        | ToolAction::Search
        | ToolAction::List
        | ToolAction::Run
        | ToolAction::Fetch
        | ToolAction::Call
        | ToolAction::Submit => action,
    };
    for action in [
        ToolAction::Read,
        ToolAction::Edit,
        ToolAction::Write,
        ToolAction::Search,
        ToolAction::List,
        ToolAction::Run,
        ToolAction::Fetch,
        ToolAction::Call,
        ToolAction::Submit,
    ]
    .map(listed)
    {
        let row = &texts(&[call(action, "src/lib.rs", None)], &BTreeMap::new())[0];
        assert_eq!(row.find("src/lib.rs"), Some(8), "{row}");
    }
}

/// The feed lists the work at a node and what the engine does there, and leaves out what it
/// keeps account of.
#[test]
fn a_feed_lists_the_work_at_a_node_and_leaves_its_accounts_out() {
    let listed = [
        call(ToolAction::Read, "src/lib.rs", None).happening,
        opened().happening,
        Happening::Session(session::happening::Happening::Refused(ToolTarget::of_path(
            Path::new("src/main.rs"),
        ))),
        reached(NodeState::Running { attempt: 2 }).happening,
        pre_check("task-1").happening,
        Happening::Scope(scope::happening::Happening::Expansion {
            task: Some("task-1".into()),
            step: Step::Requested {
                paths: vec!["src/main.rs".into()],
            },
        }),
        Happening::Artifacts(artifacts::happening::Happening::Accepted(
            ArtifactId::Interpreted {
                kind: ArtifactKind::Tasks,
            },
        )),
    ];
    for happening in &listed {
        assert!(in_feed(happening), "{happening:?}");
    }
    let left_out = [
        Happening::Session(session::happening::Happening::Message {
            kind: AgentMessageType::Usage,
            note: None,
            text: None,
            input_tokens: Some(1_000),
            output_tokens: None,
        }),
        Happening::Node(node::happening::Happening::ContextAssembled { absent: Vec::new() }),
        Happening::Node(node::happening::Happening::ScopeChecked { violations: 0 }),
        Happening::Tasks(tasks::happening::Happening::Moved {
            task: "task-1".into(),
            to: TaskStatus::Running,
        }),
        Happening::Artifacts(artifacts::happening::Happening::Submitted {
            kind: ArtifactKind::Tasks,
            name: "tasks.yaml".to_string(),
            taken: true,
        }),
    ];
    for happening in &left_out {
        assert!(!in_feed(happening), "{happening:?}");
    }
}

/// The feed lists the questions a node asks a person, and their answers once recorded, as
/// `status --follow` says each.
#[test]
fn a_feed_lists_the_questions_a_node_asks_and_their_answers() {
    let asked = Happening::Gates(gates::happening::Happening::Asked {
        questions: vec![QuestionId::from("where-global-lives")],
    });
    let answered = Happening::Gates(gates::happening::Happening::Answered {
        channel: Channel::Tty,
        responder: None,
    });
    assert!(in_feed(&asked), "{asked:?}");
    assert!(in_feed(&answered), "{answered:?}");
}

/// The heading counts the calls of the open attempt, with the stream's separator.
#[test]
fn a_feed_s_heading_counts_the_open_attempt_s_calls() {
    assert_eq!(
        feed_heading(3, Glyphs::Unicode),
        "what it is doing · 3 calls"
    );
    assert_eq!(feed_heading(1, Glyphs::Ascii), "what it is doing ; 1 call");
}

/// The run's whole chronicle lists every moment it is handed, the runner and what a session
/// spent among them, which a node's feed leaves out, each row about its node or the run and
/// saying its words as a node's feed does; its heading counts the moments, with the stream's
/// separator.
#[test]
fn the_whole_chronicle_says_every_moment_about_its_subject_and_counts_them() {
    let ran = Moment::sample(
        None,
        Happening::Run(yunta_core::events::run::happening::Happening::Resumed {
            policies: Vec::new(),
        }),
    );
    let moments = [
        ran,
        reached(NodeState::Running { attempt: 1 }),
        call(ToolAction::Read, "crates/cli/src/pack.rs", None),
    ];
    let whole = whole(&moments, &BTreeMap::new(), 2, 0, &look(Glyphs::Unicode));
    let subjects: Vec<Option<&str>> = whole
        .rows
        .iter()
        .map(|row| row.subject.as_deref())
        .collect();
    assert_eq!(
        subjects,
        [Some("run"), Some("implement"), Some("implement")]
    );
    let rows: Vec<String> = whole.rows.iter().map(|row| row.line.text()).collect();
    assert_eq!(
        rows,
        [
            "resumed",
            "running: attempt 1",
            "read    crates/cli/src/pack.rs"
        ]
    );
    assert_eq!((whole.before, whole.after), (2, 0));
    assert_eq!(
        whole_heading(104, Glyphs::Unicode),
        "its whole chronicle · 104 moments"
    );
    assert_eq!(
        whole_heading(1, Glyphs::Ascii),
        "its whole chronicle ; 1 moment"
    );
}

/// A row stays one row however narrow the pane that draws it, and the newest is strong on the
/// sixteen colors.
#[test]
fn a_row_stays_one_row_at_any_width_and_the_newest_is_strong_in_color() {
    let moments = [
        opened(),
        call(
            ToolAction::Run,
            "cargo test --workspace --test pack_global_spec global_pack_commands_use_the_global_root",
            None,
        ),
        call(ToolAction::Edit, "crates/cli/src/pack.rs", None),
    ];
    let block = Block::from(feed(
        &moments,
        &BTreeMap::new(),
        0,
        0,
        &look(Glyphs::Unicode),
    ));
    for cells in [20, 40, 60, 120] {
        let pane = Look {
            width: Width::exact(cells),
            ..look(Glyphs::Unicode)
        };
        assert_eq!(
            Terminal::on(pane).block(&block).len(),
            3,
            "at {cells} cells"
        );
    }
    let colored = Look {
        paint: Paint::ANSI16,
        ..look(Glyphs::Unicode)
    };
    let newest = Terminal::on(colored)
        .block(&block)
        .pop()
        .unwrap_or_default();
    assert_eq!(
        Paint::ANSI16.line(&newest),
        Paint::ANSI16.word(Tone::Strong, &newest.text())
    );
}
