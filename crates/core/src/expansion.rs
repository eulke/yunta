//! Every decision about a request to write outside a scope: a session's own
//! request, a location a red criterion printed, or a node's offer. The engine
//! gathers the facts; this module says what they mean, and a refusal is a
//! typed [`Denial`].

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::events::{Decider, LoggedWords};
use crate::glob::{reaches_any, scope_globset, ScopeGlob};
use crate::policy::ScopeExpansionMode;
use crate::protected::{Protected, Protection};

/// The exit a criterion's record keeps for a command that stopped before it
/// answered: the code the run's criteria record for that outcome.
const STOPPED_BEFORE_ANSWER: i32 = -2;

/// A path a request reaches, and the protection that holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Reach {
    pub path: ScopeGlob,
    pub reason: Protection,
}

/// What a decision is about: a request the session wrote, or the paths a
/// red output of the task locates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask<'a> {
    Requested {
        paths: &'a [ScopeGlob],
        reason: &'a str,
        proposal: Option<Proposal<'a>>,
    },
    /// Paths a red output of this task locates as `path:line`.
    Located {
        paths: &'a [ScopeGlob],
        criterion: &'a str,
    },
}

/// A criterion a session proposed for its request, and what running it found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proposal<'a> {
    pub cmd: &'a str,
    pub precheck: Precheck,
}

/// What running a proposed criterion found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Precheck {
    Passes,
    /// The criterion fails with this exit: the red output a request may cite.
    Red {
        exit: i32,
    },
    CouldNotRun,
    /// The criterion is not run, so no exit is known.
    NotRun,
}

impl Precheck {
    /// The exit the record keeps: none until the criterion has run.
    pub fn exit_code(self) -> Option<i32> {
        match self {
            Precheck::Passes => Some(0),
            Precheck::Red { exit } => Some(exit),
            Precheck::CouldNotRun => Some(STOPPED_BEFORE_ANSWER),
            Precheck::NotRun => None,
        }
    }
}

/// The limits a request is decided under.
#[derive(Debug, Clone, Copy)]
pub struct Bounds<'a> {
    pub mode: ScopeExpansionMode,
    /// The most files a bounded grant may touch.
    pub max_files: usize,
    pub max_per_run: Option<u32>,
    pub protected: &'a Protected,
    /// Paths another task of the batch may write: a grant there is a person's.
    pub beside: &'a [ScopeGlob],
}

/// The outcome of a request: granted, waiting for a person, or denied.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Grant {
        paths: Vec<ScopeGlob>,
        by: Decider,
    },
    Escalate {
        paths: Vec<ScopeGlob>,
        because: Waiting,
    },
    Deny(Denial),
}

/// Why a request waits for a person instead of being decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Waiting {
    /// `ask` mode: every request waits.
    ModeAsk,
    /// Another task of the batch may write one of the paths.
    SharedWithBatch,
    CapExhausted {
        granted: u32,
        cap: u32,
    },
    /// The request reaches an instruction file, which only a person grants.
    Instructions,
    /// A node's request that a person answers.
    Person,
}

/// Why a request was denied, typed. The event log keeps this value; the
/// words a reader sees are written from it where that reader is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Denial {
    ReachesProtected {
        reached: Vec<Reach>,
    },
    ModeDeny,
    NoCriterion,
    PrecheckPasses,
    CriterionCouldNotRun,
    TooManyFiles {
        touched: usize,
        bound: usize,
    },
    NothingLeft,
    NotWidenable,
    /// A person chose an option other than a grant.
    Chosen {
        option: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// A refusal read from its text, from logs written before reasons were typed.
    Legacy {
        text: LoggedWords,
    },
}

/// A request the protection, mode and batch rules leave open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub paths: Vec<ScopeGlob>,
    pub by: Decider,
    /// Whether the file bound applies: a `rules` grant is bounded, evidence
    /// under `ask` is not.
    pub bounded: bool,
}

/// The first answer a request gets, before any file of the diff is read.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Settled(Decision),
    Candidate(Candidate),
}

/// Protection, precheck, mode and batch: none of them needs git, so a request
/// they settle is never read against the worktree.
pub fn screen(ask: &Ask<'_>, bounds: &Bounds<'_>) -> Screen {
    let (paths, proposal, by, bounded) = origin(ask, bounds);
    // Protection first: no mode, batch or cap admits a path the engine never grants.
    let reached = reaching(paths, bounds.protected);
    if refuses_whole(&reached) {
        return Screen::Settled(Decision::Deny(Denial::ReachesProtected { reached }));
    }
    if let Some(denial) = precheck_denial(proposal) {
        return Screen::Settled(Decision::Deny(denial));
    }
    if let Some(decision) = by_mode(ask, bounds.mode, paths, !reached.is_empty()) {
        return Screen::Settled(decision);
    }
    if paths.iter().any(|path| reaches_any(path, bounds.beside)) {
        return Screen::Settled(Decision::Escalate {
            paths: paths.to_vec(),
            because: Waiting::SharedWithBatch,
        });
    }
    Screen::Candidate(Candidate {
        paths: paths.to_vec(),
        by,
        bounded,
    })
}

/// The paths, the proposal, who decides, and whether the file bound applies.
fn origin<'a>(
    ask: &Ask<'a>,
    bounds: &Bounds<'_>,
) -> (&'a [ScopeGlob], Option<Proposal<'a>>, Decider, bool) {
    let rules = bounds.mode == ScopeExpansionMode::Rules;
    match *ask {
        Ask::Requested {
            paths, proposal, ..
        } => (paths, proposal, Decider::Rule, rules),
        Ask::Located { paths, criterion } => (
            paths,
            None,
            Decider::Evidence {
                criterion: criterion.to_string(),
            },
            rules,
        ),
    }
}

/// The denial a proposed criterion settles on its own: a pass, or no run.
fn precheck_denial(proposal: Option<Proposal<'_>>) -> Option<Denial> {
    match proposal.map(|proposal| proposal.precheck) {
        Some(Precheck::Passes) => Some(Denial::PrecheckPasses),
        Some(Precheck::CouldNotRun) => Some(Denial::CriterionCouldNotRun),
        Some(Precheck::Red { .. } | Precheck::NotRun) | None => None,
    }
}

/// What the mode settles before any file is read: a denial, or a wait for a
/// person. `None` leaves the request a candidate.
fn by_mode(
    ask: &Ask<'_>,
    mode: ScopeExpansionMode,
    paths: &[ScopeGlob],
    reaches_instructions: bool,
) -> Option<Decision> {
    if mode == ScopeExpansionMode::Deny {
        return Some(Decision::Deny(Denial::ModeDeny));
    }
    // What is left reaches only instruction files: a person grants those under
    // ask and rules alike.
    if reaches_instructions {
        return Some(Decision::Escalate {
            paths: paths.to_vec(),
            because: Waiting::Instructions,
        });
    }
    match (ask, mode) {
        (Ask::Requested { proposal: None, .. }, ScopeExpansionMode::Rules) => {
            Some(Decision::Deny(Denial::NoCriterion))
        }
        (Ask::Requested { .. }, ScopeExpansionMode::Ask) => Some(Decision::Escalate {
            paths: paths.to_vec(),
            because: Waiting::ModeAsk,
        }),
        _ => None,
    }
}

/// The file bound, then the cap. A refusal never becomes an escalation.
pub fn settle(c: Candidate, touched: usize, granted_so_far: u32, bounds: &Bounds<'_>) -> Decision {
    if c.bounded && touched > bounds.max_files {
        return Decision::Deny(Denial::TooManyFiles {
            touched,
            bound: bounds.max_files,
        });
    }
    match bounds.max_per_run {
        Some(cap) if granted_so_far >= cap => Decision::Escalate {
            paths: c.paths,
            because: Waiting::CapExhausted {
                granted: granted_so_far,
                cap,
            },
        },
        _ => Decision::Grant {
            paths: c.paths,
            by: c.by,
        },
    }
}

/// How many of the touched files the candidate's paths cover. A set that does
/// not build counts as over any bound: the bound fails closed.
pub fn touched_under(c: &Candidate, touched: &[PathBuf]) -> usize {
    let Ok(set) = scope_globset(&c.paths) else {
        return usize::MAX;
    };
    touched.iter().filter(|file| set.is_match(file)).count()
}

/// All of `paths` when a person may grant every one of them, or the one
/// denial that stops them as a whole. The menu, the close and a chosen grant
/// of a node all decide through this function.
pub fn node_offer(
    paths: &[ScopeGlob],
    protected: &Protected,
    widenable: bool,
) -> Result<Vec<ScopeGlob>, Denial> {
    if !widenable {
        return Err(Denial::NotWidenable);
    }
    let reached = reaching(paths, protected);
    if refuses_whole(&reached) {
        return Err(Denial::ReachesProtected { reached });
    }
    if paths.is_empty() {
        return Err(Denial::NothingLeft);
    }
    Ok(paths.to_vec())
}

/// Every path a protection holds, with the strictest reason that reaches it.
fn reaching(paths: &[ScopeGlob], protected: &Protected) -> Vec<Reach> {
    paths
        .iter()
        .filter_map(|path| {
            Some(Reach {
                path: path.clone(),
                reason: protected.reached_by(path)?,
            })
        })
        .collect()
}

/// Whether a reach holds a path that no person may grant.
fn refuses_whole(reached: &[Reach]) -> bool {
    reached.iter().any(|reach| !reach.reason.person_may_grant())
}

impl Denial {
    /// Whether no work can change this refusal: a path no session of the run
    /// may write, or a mode that grants nothing. A session is told such a
    /// refusal in the call that asks; any other waits for the judgement of
    /// its work, where a red output may still locate what it asks for.
    pub fn stands(&self) -> bool {
        matches!(self, Denial::ReachesProtected { .. } | Denial::ModeDeny)
    }
}

impl fmt::Display for Denial {
    /// The code a denial is known by: its variant, spelled as the log
    /// spells it. The words of each reader are written where that reader is.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Denial::ReachesProtected { .. } => "reaches_protected",
            Denial::ModeDeny => "mode_deny",
            Denial::NoCriterion => "no_criterion",
            Denial::PrecheckPasses => "precheck_passes",
            Denial::CriterionCouldNotRun => "criterion_could_not_run",
            Denial::TooManyFiles { .. } => "too_many_files",
            Denial::NothingLeft => "nothing_left",
            Denial::NotWidenable => "not_widenable",
            Denial::Chosen { .. } => "chosen",
            Denial::Legacy { .. } => "legacy",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds<'a>(
        mode: ScopeExpansionMode,
        protected: &'a Protected,
        beside: &'a [ScopeGlob],
    ) -> Bounds<'a> {
        Bounds {
            mode,
            max_files: 5,
            max_per_run: None,
            protected,
            beside,
        }
    }

    #[test]
    fn deny_mode_grants_nothing() {
        let paths = [ScopeGlob::from("b.rs")];
        let ask = Ask::Located {
            paths: &paths,
            criterion: "cargo test",
        };
        let protected = Protected::default();
        let screened = screen(&ask, &bounds(ScopeExpansionMode::Deny, &protected, &[]));
        assert_eq!(screened, Screen::Settled(Decision::Deny(Denial::ModeDeny)));
    }

    #[test]
    fn path_of_a_concurrent_task_is_escalated() {
        let paths = [ScopeGlob::from("i/b.rs")];
        let beside = [ScopeGlob::from("i/**")];
        let ask = Ask::Located {
            paths: &paths,
            criterion: "cargo test",
        };
        let protected = Protected::default();
        let screened = screen(
            &ask,
            &bounds(ScopeExpansionMode::Rules, &protected, &beside),
        );
        let waiting = Decision::Escalate {
            paths: paths.to_vec(),
            because: Waiting::SharedWithBatch,
        };
        assert_eq!(screened, Screen::Settled(waiting));
    }

    #[test]
    fn a_request_reaching_a_protected_path_is_refused_whole() {
        let paths = [
            ScopeGlob::from("src/a.rs"),
            ScopeGlob::from(".claude/settings.json"),
        ];
        let protected = Protected::new(&[ScopeGlob::from(".claude/**")], &[]).unwrap();
        let ask = Ask::Requested {
            paths: &paths,
            reason: "edit",
            proposal: Some(Proposal {
                cmd: "cargo test",
                precheck: Precheck::Red { exit: 1 },
            }),
        };
        let reached = vec![Reach {
            path: ScopeGlob::from(".claude/settings.json"),
            reason: Protection::ProjectDeny,
        }];
        let screened = screen(&ask, &bounds(ScopeExpansionMode::Rules, &protected, &[]));
        let refused = Denial::ReachesProtected { reached };
        assert_eq!(screened, Screen::Settled(Decision::Deny(refused)));
    }

    #[test]
    fn a_node_offer_is_every_granted_path_or_the_one_denial_that_stops_them() {
        let protected = Protected::new(&[], &[]).unwrap();
        let instructions = [ScopeGlob::from("AGENTS.md")];
        assert_eq!(
            node_offer(&instructions, &protected, true),
            Ok(instructions.to_vec())
        );
        let mixed = [ScopeGlob::from("src/a.rs"), ScopeGlob::from(".git/config")];
        assert!(matches!(
            node_offer(&mixed, &protected, true),
            Err(Denial::ReachesProtected { .. })
        ));
        assert_eq!(
            node_offer(&instructions, &protected, false),
            Err(Denial::NotWidenable)
        );
        assert_eq!(node_offer(&[], &protected, true), Err(Denial::NothingLeft));
    }
}
