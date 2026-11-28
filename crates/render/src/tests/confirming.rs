//! What a confirmation asks before an act, in the design's words: closing runs, with the
//! runs a close leaves open named under its sentence, stopping one, and setting a repository
//! up with its pack, which says what the write touches as the note once written says it.

use yunta_core::refusal::CloseRefusal;

use super::screen::spelled;
use crate::screen::{wrote_the_setup, Confirming, LeftOpen, WhyLeftOpen};

/// Closing a run asks it by its handle, says in the design's first sentence what closing does
/// and in a second what a close leaves, its branch staying and its checkouts given back, and
/// names the keys that close it or keep it; closing two asks it of both, every word of the
/// sentence and of the keys agreeing with them.
#[test]
fn closing_runs_asks_in_the_designs_words() {
    let one = Confirming::Close {
        runs: vec!["EZRPE9".to_string()],
        left: Vec::new(),
    };
    assert_eq!(one.question(), "close EZRPE9?");
    assert_eq!(
        one.sentence(),
        "It closes as cancelled, by you, and stops waiting on a person. Its branch stays; its \
         checkouts go back to the project."
    );
    assert_eq!(spelled(&one.legend()), "enter close it · esc keep it");
    let runs = vec!["EZRPE9".to_string(), "M7WQY4".to_string()];
    let two = Confirming::Close {
        runs,
        left: Vec::new(),
    };
    assert_eq!(two.question(), "close EZRPE9 and M7WQY4?");
    assert_eq!(
        two.sentence(),
        "They close as cancelled, by you, and stop waiting on a person. Their branches stay; \
         their checkouts go back to the project."
    );
    assert_eq!(spelled(&two.legend()), "enter close them · esc keep them");
}

/// A close names under its sentence each run it was asked to close and leaves open, a line
/// each, with why: what a close of it refuses, in the words `yunta close` refuses with, or what
/// holds it; any other act leaves nothing undone.
#[test]
fn a_close_names_each_run_it_leaves_open_with_why() {
    let left = vec![
        LeftOpen {
            run: "Q78GRR".to_string(),
            why: WhyLeftOpen::Refused(CloseRefusal::Driven),
        },
        LeftOpen {
            run: "0RPHAN".to_string(),
            why: WhyLeftOpen::Holding("its event log does not read back".to_string()),
        },
    ];
    let closing = Confirming::Close {
        runs: vec!["EZRPE9".to_string()],
        left,
    };
    assert_eq!(
        closing.left_undone(),
        [
            "Q78GRR is not closed: an engine is driving the run.",
            "0RPHAN is not closed: its event log does not read back."
        ]
    );
    let stopping = Confirming::Stop {
        run: "Q78GRR".to_string(),
    };
    assert!(stopping.left_undone().is_empty());
}

/// Stopping a run asks it by its handle, says it stops now and waits until it is continued
/// or closed, and names the keys that stop it or keep it going, as the design's box does;
/// a question names one run bare, and three with commas before the last `and`.
#[test]
fn stopping_a_run_asks_in_the_designs_words() {
    let stopping = Confirming::Stop {
        run: "Q78GRR".to_string(),
    };
    assert_eq!(stopping.question(), "stop Q78GRR?");
    assert_eq!(
        stopping.sentence(),
        "It stops now, with every session under it, and waits where it stopped until you \
         continue it or close it."
    );
    assert_eq!(spelled(&stopping.legend()), "enter stop it · esc keep it");
    let one = Confirming::Close {
        runs: vec!["ST0PPD".to_string()],
        left: Vec::new(),
    };
    assert_eq!(one.question(), "close ST0PPD?");
    let three = Confirming::Close {
        runs: ["A", "B", "C"].map(String::from).to_vec(),
        left: Vec::new(),
    };
    assert_eq!(three.question(), "close A, B and C?");
}

/// Every path a setup writes, by its path in the repository, as setting it up names them.
fn written() -> Vec<String> {
    [
        ".yunta/config.yaml",
        ".gitignore",
        ".yunta/skills/yunta-mechanism/SKILL.md",
    ]
    .map(String::from)
    .to_vec()
}

/// Setting a repository up with its pack asks to install it, says the configuration is written
/// and the pack installed, and names what it touches outside `.yunta/`; `esc` writes it without
/// the pack. Once written, the note says the same of what was: `.gitignore` named only where it
/// was written, the pack only where it was installed.
#[test]
fn setting_up_with_the_pack_says_what_it_touches_before_and_once_written() {
    let asks = Confirming::SetUpWith {
        pack: "yunta/fragua".to_string(),
        writes: written(),
    };
    assert_eq!(asks.question(), "install yunta/fragua?");
    assert_eq!(
        asks.sentence(),
        "It writes .yunta/config.yaml and installs yunta/fragua. Nothing outside .yunta/ is \
         touched but .gitignore."
    );
    assert_eq!(
        spelled(&asks.legend()),
        "enter install it · esc write without it"
    );
    assert!(asks.left_undone().is_empty());
    assert_eq!(
        wrote_the_setup(Some("yunta/fragua"), &written()),
        "Wrote .yunta/config.yaml and installed yunta/fragua. Nothing outside .yunta/ was \
         touched but .gitignore."
    );
    let ignored_already: Vec<String> = written()
        .into_iter()
        .filter(|path| path != ".gitignore")
        .collect();
    assert_eq!(
        wrote_the_setup(Some("yunta/fragua"), &ignored_already),
        "Wrote .yunta/config.yaml and installed yunta/fragua. Nothing outside .yunta/ was \
         touched."
    );
    assert_eq!(
        wrote_the_setup(None, &written()),
        "Wrote .yunta/config.yaml. Nothing outside .yunta/ was touched but .gitignore."
    );
}

/// Removing a pack, installing one that ships executors, opening a session per binding and
/// collecting what a dry run found each ask their act by what it acts on, say what it does,
/// and name the keys that do it or keep things as they are; none leaves anything undone.
#[test]
fn the_packs_the_sessions_and_collecting_ask_in_their_words() {
    let removing = Confirming::RemovePack {
        pack: "yunta/fragua".to_string(),
    };
    assert_eq!(removing.question(), "remove yunta/fragua?");
    assert_eq!(
        removing.sentence(),
        "Its files and its lock entry go from this project."
    );
    assert_eq!(spelled(&removing.legend()), "enter remove it · esc keep it");
    let installing = Confirming::InstallPack {
        pack: "yunta/fragua".to_string(),
        executors: ["lint.sh", "ship.sh"].map(String::from).to_vec(),
    };
    assert_eq!(installing.question(), "install yunta/fragua?");
    assert_eq!(
        installing.sentence(),
        "It ships executors it runs: `lint.sh`, `ship.sh`."
    );
    assert_eq!(
        spelled(&installing.legend()),
        "enter install it · esc go back"
    );
    let sessions = Confirming::OpenSessions;
    assert_eq!(sessions.question(), "open a session per binding?");
    assert_eq!(sessions.sentence(), "It spends a prompt each.");
    assert_eq!(spelled(&sessions.legend()), "enter open them · esc go back");
    let collecting = Confirming::Collect;
    assert_eq!(collecting.question(), "collect what the dry run found?");
    assert_eq!(collecting.sentence(), "What it lists is removed for good.");
    assert_eq!(
        spelled(&collecting.legend()),
        "enter collect it · esc keep it"
    );
    for confirming in [removing, installing, sessions, collecting] {
        assert!(confirming.left_undone().is_empty(), "{confirming:?}");
    }
}

/// What `yunta close` answers with says what the close left in the words its confirmation
/// says it leaves, done: the checkouts given back, the branch staying; nothing of `gc`, which
/// never removes a run's branch.
#[test]
fn a_close_s_page_says_what_the_close_left() {
    use surco::surface::{Markdown, Surface};
    use surco::words::Standing;

    use crate::page::{Closed, Page};

    let run_id = yunta_core::RunId::from_static("01M7SMA11RVN0000000EZRPE9");
    let by = yunta_core::Responder::from_static("lead");
    let page = Closed {
        run_id: &run_id,
        word: Standing::Stopped,
        by: &by,
    };
    let drawn = Markdown.draw(&page.document(&Markdown.look()));
    assert!(
        drawn.contains(
            "closed by lead: its checkouts went back to the project, and its branch stays"
        ),
        "{drawn}"
    );
    assert!(!drawn.contains("gc"), "{drawn}");
}
