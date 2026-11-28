//! The cut every target screen stands on, by the target's name: the runs it shows, each at
//! a moment of its log, where the application opens over them, what the person points at
//! and the keys pressed from there; or the place of the test home it opens in
//! ([`crate::home`]).
//!
//! The design's run stands at the moments its frames are drawn at: `plan` at work (42),
//! its tasks accepted (53), the round asked (19), the plan's gate (78), the scope asked
//! (118) and answered (120), `implement` at work (99, 104), three reviewers at work with
//! the four findings posted and none answered (200), two findings answered (225), `tests`
//! at work (250), failed and asked about at the terminal of the engine that drives it
//! (251), its failure asked again once the tree moved (254), `ship` (263), where its
//! numbers and its graph stand too, and its end (276). A small run stands at its end, but
//! where its scene is a moment before: two tasks at work at once (24), `plan` at work again
//! after it was sent back (25), the round asked again once its answers were refused (14),
//! and a question open at the terminal of the engine that asks it, before its answer (6).
//! A run at work is driven by an engine of the test's process; a parked or ended run has
//! none.
//!
//! Keys that record an answer leave the cut at the pause, and say where the log then goes
//! on to ([`TargetCut::goes_on_to`]): the answer on the log, then the run on to its next
//! node's start, `implement` again (123), `tests` again (258) and `pr` (273), driven by the
//! engine it is handed on to. A screen of a run the person leads is reached by the keys
//! that take the lead, moving along its nodes; where the run then closes promoted, the
//! cut stands before the close (8), and its log goes on to it (9) once the person took the
//! lead, so its successor comes while the person holds the screen.
//!
//! Every run's screen stands in a field: beside the run in view, the two older runs of
//! [`BESIDE`], one that needs the person and one stalled, which its tally counts and its
//! attention row reads. The field's targets share one field, as the design draws it: the
//! design's run asking at its terminal at `tests`, then the runs that need the person and
//! the one stalled the design lists first, and a run in each group it does not draw. `r`
//! is pressed there on a run a decision holds, `EZRPE9`, and on one whose pause no option
//! answers, `C0NS0E`, which it resumes.
//!
//! A new run's targets, and a workflow's history and graph, open in the home's repository
//! set up as the design's, whose runs are [`WITH_FRAGUA`]; a setup's, in the design's
//! repository before its setup, the pack it installs in reach or out of it. Their keys take
//! the workflows in the design's order, `yunta/fragua` first, then `lint-fix` and
//! `run-tasks`. A run's handle is its id's last six characters.

use super::{Built, Shown};

/// A target screen's cut, by the target's name: what it shows and how the person came
/// there. Not [`super::Cut`], the store a target's cut loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetCut {
    /// The target's name, as its frames are named.
    pub name: &'static str,
    /// The runs it shows, and where the application opens over them; or the place of the
    /// test home it opens in, which holds what it shows.
    pub stands: Stands,
    /// What the person points at, taken from the run: a node of the run in view, or a run
    /// of the field by its handle. `None` while the run leads.
    pub pointed: Option<Pointed>,
    /// The keys pressed, in order, once the screen opens: each as the design's recording
    /// names a key (a letter or a digit, `enter`, `esc`, `tab`, `space`, `up`, `down`, `/`,
    /// `?`), or `type:` and the words typed.
    pub keys: &'static [&'static str],
    /// Whether what the run in view asks has been handed to the screen.
    pub asked: Asked,
    /// The seq the log of the run in view goes on to once the keys are pressed: they
    /// answer, the recording's own answer recorded, and the engine the run is handed on to
    /// takes it to its next node's start; or they take the lead, and the run then closes.
    /// `None` where the keys leave the log as it stands.
    pub goes_on_to: Option<u64>,
    /// Where the application opens before the keys are pressed.
    pub opens: Opens,
}

/// Where the application opens in a target's place before the target's keys are pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opens {
    /// As bare `yunta` opens there.
    Bare,
    /// On the workflows, over where bare `yunta` stands, as the application opened on the
    /// step that lists them opens them: in a repository set up with no run, over the first
    /// request, where every key types. Not `esc` then `w` there, which reach them over the
    /// field the first request gives way to.
    OnTheWorkflows,
}

/// Where a target's screen opens, over the runs it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stands {
    /// The screen of the run shown, in its field: the runs of [`BESIDE`] beside it, in its
    /// store.
    Run(Shown),
    /// The field over every run shown.
    Field(&'static [Shown]),
    /// A place of the test home, which holds what the target shows.
    Home(HomePlace),
}

impl Stands {
    /// Every run a cut of it loads: a run's screen's run first, then the runs beside it; a
    /// field's runs; none for a place of the home, which holds its own.
    pub fn runs(self) -> Vec<Shown> {
        match self {
            Stands::Run(shown) => std::iter::once(shown).chain(BESIDE).collect(),
            Stands::Field(shown) => shown.to_vec(),
            Stands::Home(_) => Vec::new(),
        }
    }
}

/// A place of the test home a target opens in, as [`crate::home::Home`] builds it and
/// [`crate::home::Home::workspace_at`] opens it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomePlace {
    /// A directory in no repository: `Home::outside`.
    Outside,
    /// The design's repository before its setup, a Rust workspace whose origin is on GitHub,
    /// never set up (`DesignsPlaces::setting_up`); a `claude` and a `codex` answering on its
    /// `PATH` (`Home::clis`, `DesignsPlaces::codex`), and its git pointing the Yunta
    /// repository's URL at the pack a setup installs (`Home::reference_pack`).
    SettingUp,
    /// The same repository, its git pointing the Yunta repository's URL at a directory no
    /// repository stands in: the pack a setup installs, out of reach.
    PackOutOfReach,
    /// A repository set up with its runners on both adapters this binary builds, one
    /// workflow, a case of it and no run: `Home::set_up`.
    SetUp,
    /// A repository set up as the design's, `DesignsPlaces::with_fragua`: `yunta/fragua`
    /// installed as a pack beside its own `lint-fix` and `run-tasks`, which reads a tasks
    /// document beside its request; three finished runs of fragua, the design's among them; a
    /// run that needs the person and one stalled. A `claude` and a `codex` answer on its
    /// `PATH`.
    WithFragua,
    /// A repository with a pack installed: `Home::packed`.
    Packed,
    /// A repository set up with runs, one finished and one parked, no runner declared, git
    /// alone on its `PATH` (no agent CLI, no C compiler), its checkouts keeping a budget a
    /// disk of any size reads alike: `Home::worked`.
    Worked,
}

/// What the person points at, taken from the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointed {
    /// A node of the run in view, by its id.
    Node(&'static str),
    /// A run of the field, by its handle.
    Run(&'static str),
}

/// Whether what the run in view asks has been handed to the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// The follower read it and handed it, as it does once for each pause.
    Handed,
    /// The pause is on the log and what it asks is not handed yet.
    NotYet,
}

impl TargetCut {
    /// The screen of `shown`'s run, in its field.
    const fn run(name: &'static str, shown: Shown) -> TargetCut {
        TargetCut::standing(name, Stands::Run(shown))
    }

    /// The field over `shown`.
    const fn field(name: &'static str, shown: &'static [Shown]) -> TargetCut {
        TargetCut::standing(name, Stands::Field(shown))
    }

    /// Where the application opens in `place`.
    const fn home(name: &'static str, place: HomePlace) -> TargetCut {
        TargetCut::standing(name, Stands::Home(place))
    }

    /// `stands`, nothing pointed, no key pressed, what it asks handed, the log left as it
    /// stands.
    const fn standing(name: &'static str, stands: Stands) -> TargetCut {
        TargetCut {
            name,
            stands,
            pointed: None,
            keys: &[],
            asked: Asked::Handed,
            goes_on_to: None,
            opens: Opens::Bare,
        }
    }

    /// Opened on the workflows, over where bare `yunta` stands.
    const fn on_the_workflows(self) -> TargetCut {
        TargetCut {
            opens: Opens::OnTheWorkflows,
            ..self
        }
    }

    /// Where the screen stands once its keys are pressed and the run went on: the run in
    /// view at [`TargetCut::goes_on_to`], driven by an engine alive, beside the runs it
    /// stood beside. `None` where the keys leave the log as it stands, or the screen shows
    /// no one run.
    pub fn gone_on(&self) -> Option<Stands> {
        let seq = self.goes_on_to?;
        match self.stands {
            Stands::Run(shown) => Some(Stands::Run(Shown { seq, ..shown }.alive())),
            Stands::Field(_) | Stands::Home(_) => None,
        }
    }

    /// With `keys` pressed.
    const fn pressing(self, keys: &'static [&'static str]) -> TargetCut {
        TargetCut { keys, ..self }
    }

    /// With the node `node` pointed at by the person.
    const fn at_node(self, node: &'static str) -> TargetCut {
        TargetCut {
            pointed: Some(Pointed::Node(node)),
            ..self
        }
    }

    /// With the field's run `handle` pointed at.
    const fn at_run(self, handle: &'static str) -> TargetCut {
        TargetCut {
            pointed: Some(Pointed::Run(handle)),
            ..self
        }
    }

    /// Before what the run asks is handed.
    const fn not_yet_asked(self) -> TargetCut {
        TargetCut {
            asked: Asked::NotYet,
            ..self
        }
    }

    /// Its keys recording an answer, the log of the run in view going on to `seq`.
    const fn going_on_to(self, seq: u64) -> TargetCut {
        TargetCut {
            goes_on_to: Some(seq),
            ..self
        }
    }
}

/// The design's run, cut at `seq`.
const fn design(seq: u64) -> Shown {
    Shown::at(Built::Designed, seq)
}

/// The design's run with `plan` at work, three calls made.
const AT_WORK: Shown = design(42).alive();
/// `plan` once its tasks are accepted and submitted.
const TASKS_ACCEPTED: Shown = design(53).alive();
/// `tests`, the suite, at work.
const CHECK_AT_WORK: Shown = design(250).alive();
/// The round `grill` asks.
const ROUND: Shown = design(19);
/// The plan's gate.
const PLANS_GATE: Shown = design(78);
/// The scope `implement`'s task asks for.
const SCOPE: Shown = design(118);
/// The scope granted, the run not yet woken.
const SCOPE_ANSWERED: Shown = design(120);
/// `implement` at work on its task, eight calls made.
const BUILDING: Shown = design(99).alive();
/// `implement` four calls on.
const BUILDING_ON: Shown = design(104).alive();
/// Three reviewers at work, the four findings posted, the one that blocks among them, none
/// answered yet.
const REVIEWING: Shown = design(200).alive();
/// `fix-findings` at work, two of the four findings answered.
const ANSWERING: Shown = design(225).alive();
/// The suite's failure, asked again once the tree moved.
const FAILURE: Shown = design(254);
/// The suite failed, asked about at the terminal of the engine that drives the run.
const ASKING_AT_TESTS: Shown = design(251).asking("tests");
/// The last gate.
const SHIP: Shown = design(263);
/// The run finished.
const FINISHED: Shown = design(276);

/// Where the design's run goes on to once its scope is granted: `implement` started again,
/// the session picked back up with the path granted.
const IMPLEMENT_AGAIN: u64 = 123;
/// Where it goes on to once its failure is answered `retry`: `tests` started again on the
/// tree the person edited.
const TESTS_AGAIN: u64 = 258;
/// Where it goes on to once its last gate is approved: `pr` started.
const PR_STARTED: u64 = 273;

/// The runs every run's screen stands beside in its field, older than every run a screen
/// shows: the first of each kind the design's own field lists, one that needs the person,
/// its `grill` failed forty days before the design's run, and one stalled eleven days. The
/// tally counts them, and the attention row names the one that waits.
pub const BESIDE: [Shown; 2] = [
    Shown::at(Built::NotLoggedIn, 7),
    Shown::at(Built::LongStalled, 2).gone(),
];

/// The field as the design draws it, with a run in each group it draws: the design's run
/// asking at its own terminal at `tests`, alone asking now; the runs that need the person,
/// the design's own `EZRPE9` the oldest, whose mode leaves `spec` out; `EKPBGE` stalled;
/// and where the design draws none, runs stopped, one running, closed and unreadable, and
/// one of another repository.
const FIELD: &[Shown] = &[
    ASKING_AT_TESTS,
    BESIDE[0],
    Shown::at(Built::ConsoleGate, 5),
    Shown::at(Built::TokenCap, 9),
    Shown::at(Built::Blocked, 6),
    BESIDE[1],
    Shown::at(Built::Stopped, 3),
    Shown::at(Built::Aborted, 16),
    Shown::at(Built::TwoTasks, 24).alive(),
    Shown::at(Built::Reported, 14),
    Shown::at(Built::Failed, 5),
    Shown::at(Built::Orphans, 4).unread(),
    Shown::at(Built::Created, 1).elsewhere(),
];

/// The runs of the home's repository set up as the design's: two finished runs of fragua
/// and the design's own, its third; a gate with no plan, which needs the person; and a
/// stalled run.
pub const WITH_FRAGUA: &[Shown] = &[
    Shown::at(Built::Past, 72),
    design(276),
    Shown::at(Built::ConsoleGate, 5),
    Shown::at(Built::Stalled, 2).gone(),
];

/// What a new run's targets type as the request.
const REQUEST: &str = "type:add support for installing a pack globally";

/// What a round's question answered by typing is answered with: the release's name.
const RELEASE_TYPED: &str = "type:Spring release";

/// Every target screen's cut, by the target's name: a run at work, a decision in place,
/// where a run stands, what stands over a run, the field, the first request and the field
/// with no run it gives way to, a new run and a setup, and the pages the design draws no
/// frame for.
pub const TARGET_CUTS: &[TargetCut] = &[
    // A run at work.
    TargetCut::run("work", AT_WORK),
    TargetCut::run("work-moved", TASKS_ACCEPTED),
    TargetCut::run("work-command", CHECK_AT_WORK),
    TargetCut::run("created", Shown::at(Built::Created, 1)),
    TargetCut::run("still-answered", SCOPE_ANSWERED),
    TargetCut::run("still-unread", SCOPE).not_yet_asked(),
    TargetCut::run("wakes-alone", Shown::at(Built::WakesAlone, 7)),
    TargetCut::run("board", BUILDING),
    TargetCut::run("board-moved", BUILDING_ON),
    TargetCut::run("board-tasks", Shown::at(Built::TwoTasks, 24).alive()),
    TargetCut::run("findings", REVIEWING),
    TargetCut::run("findings-answered", ANSWERING),
    TargetCut::run("feed-back", BUILDING_ON).pressing(&["up", "up", "up"]),
    TargetCut::run("node-working", AT_WORK).at_node("plan"),
    TargetCut::run("node-needs-you", SCOPE).pressing(&["esc"]),
    TargetCut::run("node-ended", AT_WORK).at_node("brief"),
    TargetCut::run("node-never-ran", AT_WORK).at_node("spec"),
    TargetCut::run("node-sent-back", Shown::at(Built::SentBack, 25).alive()).at_node("plan"),
    TargetCut::run("node-skipped", Shown::at(Built::ModeSkips, 7)).at_node("extra"),
    // A decision in place.
    TargetCut::run("round", ROUND),
    TargetCut::run("round-answered", ROUND).pressing(&["enter"]),
    TargetCut::run("round-refused", Shown::at(Built::Refused, 14)).pressing(&[RELEASE_TYPED]),
    TargetCut::run("round-aside", ROUND).pressing(&["esc"]),
    TargetCut::run("desk", PLANS_GATE),
    TargetCut::run("desk-confirm", PLANS_GATE).pressing(&["1"]),
    TargetCut::run("desk-words", PLANS_GATE).pressing(&["2", "enter"]),
    TargetCut::run("desk-proof", PLANS_GATE).pressing(&["p"]),
    TargetCut::run("desk-decided", PLANS_GATE).pressing(&["d"]),
    TargetCut::run("desk-change", PLANS_GATE).pressing(&["k"]),
    TargetCut::run("card", Shown::at(Built::NotesGate, 11)),
    TargetCut::run("card-words", SCOPE).pressing(&["n"]),
    TargetCut::run("card-recorded", SHIP)
        .pressing(&["enter"])
        .going_on_to(PR_STARTED),
    TargetCut::run("card-unread", Shown::at(Built::Unread, 25)),
    TargetCut::run("failure", FAILURE),
    TargetCut::run("failure-recorded", FAILURE)
        .pressing(&["enter"])
        .going_on_to(TESTS_AGAIN),
    TargetCut::run("scope", SCOPE),
    TargetCut::run("scope-recorded", SCOPE)
        .pressing(&["enter"])
        .going_on_to(IMPLEMENT_AGAIN),
    TargetCut::run("departure", Shown::at(Built::Departure, 25)),
    TargetCut::run("budget", Shown::at(Built::TokenCap, 9)),
    TargetCut::run("loop-cap", Shown::at(Built::LoopCap, 35)),
    TargetCut::run("ship", SHIP),
    // Where a run stands.
    TargetCut::run("standstill", Shown::at(Built::NoMenu, 7)),
    TargetCut::run("no-room", Shown::at(Built::NoRoom, 2)),
    TargetCut::run("orphans", Shown::at(Built::Orphans, 4)),
    TargetCut::run("stopped", Shown::at(Built::Stopped, 3)),
    TargetCut::run("stalled", Shown::at(Built::Stalled, 2).gone()),
    TargetCut::run("pull-request", Shown::at(Built::PullRequest, 6)),
    TargetCut::run("child", Shown::at(Built::Child, 4)),
    TargetCut::run(
        "asked-elsewhere",
        Shown::at(Built::AskedElsewhere, 6).asking("ship"),
    ),
    TargetCut::run("receipt", FINISHED),
    TargetCut::run("reported", Shown::at(Built::Reported, 14)),
    TargetCut::run("failed", Shown::at(Built::Failed, 5)),
    TargetCut::run("promoted", Shown::at(Built::Promoted, 8))
        .pressing(&["j", "k"])
        .going_on_to(9),
    TargetCut::run("broken", Shown::at(Built::Broken, 3)),
    // What stands over a run.
    TargetCut::run("palette", AT_WORK).pressing(&["/"]),
    TargetCut::run("palette-narrowed", AT_WORK).pressing(&["/", "type:st"]),
    TargetCut::run("palette-aside", ROUND).pressing(&["esc", "/"]),
    TargetCut::run("receipt-palette", FINISHED).pressing(&["/"]),
    TargetCut::run("help", AT_WORK).pressing(&["?"]),
    TargetCut::field("field-palette", FIELD).pressing(&["/"]),
    TargetCut::field("field-help", FIELD).pressing(&["?"]),
    TargetCut::run("stats", SHIP).pressing(&["s"]),
    TargetCut::run("graph", SHIP).pressing(&["g"]),
    TargetCut::run("close-run", Shown::at(Built::Stopped, 3)).pressing(&["x"]),
    TargetCut::run("cancel-run", AT_WORK).pressing(&["c"]),
    TargetCut::run("chronicle", BUILDING_ON).pressing(&["/", "type:chronicle", "enter"]),
    // The field.
    TargetCut::field("field", FIELD),
    TargetCut::field("field-closed", FIELD).pressing(&["d"]),
    TargetCut::field("field-machine", FIELD).pressing(&["e"]),
    TargetCut::field("field-unreadable", FIELD).at_run("0RPHAN"),
    TargetCut::field("field-marked", FIELD).pressing(&["space", "j", "space"]),
    TargetCut::field("field-close", FIELD).pressing(&["space", "j", "space", "x"]),
    TargetCut::field("field-note", FIELD)
        .at_run("EZRPE9")
        .pressing(&["r"]),
    TargetCut::field("field-resumed", FIELD)
        .at_run("C0NS0E")
        .pressing(&["r"]),
    TargetCut::field("field-glance-stalled", FIELD).at_run("EKPBGE"),
    TargetCut::field("field-glance-stopped", FIELD).at_run("ST0PPD"),
    // The first request, the field `esc` leaves it for with no run in it, a new run and a
    // setup.
    TargetCut::home("first-request", HomePlace::SetUp),
    TargetCut::home("field-empty", HomePlace::SetUp).pressing(&["esc"]),
    TargetCut::home("new-run", HomePlace::WithFragua).pressing(&["n", REQUEST]),
    TargetCut::home("new-run-mode", HomePlace::WithFragua).pressing(&["n", REQUEST, "tab", "tab"]),
    TargetCut::home("new-run-inputs", HomePlace::WithFragua)
        .pressing(&["n", REQUEST, "tab", "down", "down"]),
    TargetCut::home("new-run-refused", HomePlace::WithFragua)
        .pressing(&["n", REQUEST, "tab", "down", "down", "enter"]),
    TargetCut::home("setup", HomePlace::SettingUp),
    TargetCut::home("setup-changing", HomePlace::SettingUp).pressing(&["k", "enter"]),
    TargetCut::home("setup-audit", HomePlace::SettingUp).pressing(&["enter"]),
    TargetCut::home("setup-confirm", HomePlace::SettingUp).pressing(&["enter", "enter"]),
    TargetCut::home("setup-written", HomePlace::SettingUp).pressing(&["enter", "enter", "enter"]),
    TargetCut::home("setup-unreachable", HomePlace::PackOutOfReach),
    // The pages the design draws no frame for.
    TargetCut::home("machine", HomePlace::Worked).pressing(&["m"]),
    TargetCut::home("upkeep", HomePlace::Worked).pressing(&["m", "u"]),
    TargetCut::home("workflows", HomePlace::SetUp).on_the_workflows(),
    TargetCut::home("workflow-checked", HomePlace::SetUp)
        .on_the_workflows()
        .pressing(&["c"]),
    TargetCut::home("workflow-history", HomePlace::WithFragua).pressing(&["w", "h"]),
    TargetCut::home("workflow-graph", HomePlace::WithFragua).pressing(&["w", "g"]),
    TargetCut::home("cases", HomePlace::SetUp)
        .on_the_workflows()
        .pressing(&["t"]),
    TargetCut::home("new-workflow", HomePlace::SetUp)
        .on_the_workflows()
        .pressing(&["n"]),
    TargetCut::home("packs", HomePlace::Packed)
        .on_the_workflows()
        .pressing(&["p"]),
    TargetCut::home("pack-audited", HomePlace::Packed)
        .on_the_workflows()
        .pressing(&["p", "a"]),
    TargetCut::home("document-shape", HomePlace::SetUp)
        .on_the_workflows()
        .pressing(&["s"]),
    TargetCut::run("evidence-receipt", FINISHED).pressing(&["e"]),
    TargetCut::run("evidence-verified", FINISHED).pressing(&["v"]),
    TargetCut::run("evidence-page", FINISHED).pressing(&["o"]),
    TargetCut::home("signpost", HomePlace::Outside),
];
