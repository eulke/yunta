//! What a run is cautioned about ahead of its start, in the words a reader
//! sees. A caution is data in `yunta_core`: what `check` found and what
//! the run would get through regardless. Every surface that says one calls
//! [`line()`], so none of them can say a different thing.

use yunta_core::caution::{Caution, MissingContextFile, PromotedRead};
use yunta_core::text::counted;
use yunta_core::units::Tokens;
use yunta_core::workflow::Place;
use yunta_core::{ModeName, NodeId};

/// What could put a missing `files:` path where its reader looks:
/// what tells a refusal from a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// A node that runs ahead of the reader might write the file.
    EarlierNodeMay,
    /// No node that runs ahead of the reader can write the file.
    NothingEarlierCan,
}

/// The one line a reader sees for a caution.
pub fn line(caution: &Caution) -> String {
    match caution {
        Caution::BudgetBelowHistory { cap, p90, samples } => budget(*cap, *p90, *samples),
        Caution::UndeclaredParallelScope { group } => parallel_scope(group),
        Caution::UndeclaredFanOutScope { nodes: list } => fan_out(list),
        Caution::PushToBaseWithoutGate { node, branch } => push_to_base(node, branch),
        Caution::BaselineNeverCompared { suite } => baseline_unread(suite),
        Caution::ReadOnlyThroughPromotion { place, what, mode } => {
            read_only_through_promotion(place, what, mode)
        }
        Caution::OptionalNeedsNothing { node } => optional_needs_nothing(node),
        Caution::ProgramNotOnPath { node, program } => program_not_on_path(node, program),
        Caution::ContextFileMissing {
            node,
            path,
            base,
            missing,
        } => context_file(node, path, base.as_deref(), missing),
        Caution::TokenCapAtSessionEnd { node } => token_cap_at_session_end(node),
    }
}

/// The declared cap sits under what history says the workflow spends at p90.
fn budget(cap: u64, p90: f64, samples: usize) -> String {
    format!(
        "`limits.max_tokens_per_run` ({cap}) is below this workflow's historical p90 ({} over {}) — \
         the run may pause on its budget",
        Tokens::rounded(p90).exact(),
        counted(samples, "run")
    )
}

/// Children that can write without declared scope, in one parallel group.
fn parallel_scope(group: &NodeId) -> String {
    format!(
        "parallel group `{group}`: two or more children can write and don't declare scope as \
         disjoint — they share the run's tree, so the last write wins and nothing says so; \
         declare `scope` on each, which gives every child a checkout of its own and its \
         writes a boundary this engine enforces"
    )
}

/// Scopeless nodes that can all write at once, in the list they name.
fn fan_out(list: &[NodeId]) -> String {
    format!(
        "nodes {} have no dependency paths between them and can all write without declared \
         scope — with `max_parallel_nodes` > 1 they share the run's tree at the same moment \
         and the last write wins; declare `scope` on each, which gives every one a checkout \
         of its own, or chain them with `depends_on`",
        nodes(list)
    )
}

/// A literal push to the base branch with no gate ahead of it.
fn push_to_base(node: &NodeId, branch: &str) -> String {
    format!(
        "node `{node}` pushes to the base branch (`{branch}`) with no gate anywhere before it \
         in the DAG — put a gate ahead of the push, or push to `{{{{run.branch}}}}`"
    )
}

/// A baseline suite that no node of the workflow compares.
fn baseline_unread(suite: &str) -> String {
    format!(
        "config declares `baseline.suite` (`{suite}`) and no node of this workflow or of the \
         workflows it composes is a `baseline_compare`: run on its own, this workflow measures \
         the suite before its first node and nothing reads the measurement — add the check, \
         or drop the suite"
    )
}

/// A read that only a promotion into the mode holds.
fn read_only_through_promotion(place: &Place, what: &PromotedRead, mode: &ModeName) -> String {
    format!(
        "{} reads {}, which mode `{mode}` holds only when a run is promoted into it from an \
         earlier mode — a run started with `--mode {mode}` is refused for it",
        crate::place::label(place),
        promoted(what)
    )
}

/// An `optional` node that needs nothing the project declares.
fn optional_needs_nothing(node: &NodeId) -> String {
    format!(
        "node `{node}` is declared `optional` and needs nothing the project declares — no \
         command, forge or other key — so no run leaves it out; drop `optional`"
    )
}

/// A literal command that starts a program this machine does not have.
fn program_not_on_path(node: &NodeId, program: &str) -> String {
    format!(
        "node `{node}` runs `{program}`, which is not on this machine's `PATH` — install it \
         before the run reaches `{node}`, or it stops there"
    )
}

/// A `files:` path a node reads that the tree a run starts from does not hold.
fn context_file(
    node: &NodeId,
    path: &str,
    base: Option<&str>,
    missing: &MissingContextFile,
) -> String {
    missing_sentence(node, path, base, missing, Reach::EarlierNodeMay)
}

/// The token cap is checked when a session ends, because the adapters report
/// no usage mid-session.
fn token_cap_at_session_end(node: &NodeId) -> String {
    format!(
        "node `{node}`: `limits.max_tokens_per_run` is checked when the session ends, because \
         no candidate adapter declares `mid_session_token_cap` and so reports no usage \
         mid-session — a session can spend past the cap before the check; use an adapter that \
         declares `mid_session_token_cap` to check the cap as the session runs"
    )
}

/// What a mode reads that only a promotion into it holds, in the words a
/// reader sees: the artifact of the run, or the node it reads from.
pub fn promoted(what: &PromotedRead) -> String {
    match what {
        PromotedRead::Artifact(artifact) => format!("the run's {}", artifact.label()),
        PromotedRead::Source(node) => format!("from `{node}`"),
    }
}

/// The nodes a fan-out caution names: each in backticks, in the order given,
/// joined by commas.
pub fn nodes(nodes: &[NodeId]) -> String {
    nodes
        .iter()
        .map(|node| format!("`{node}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The sentence for a missing `files:` path: what is missing, why the run
/// would not see it, whether anything could write it, and what to
/// do — each shape its own remedy.
pub fn missing_sentence(
    node: &NodeId,
    path: &str,
    base: Option<&str>,
    missing: &MissingContextFile,
    reach: Reach,
) -> String {
    use MissingContextFile as M;
    let reads = format!("node `{node}` reads `{path}` (a `files:` context source)");
    let stops = match reach {
        Reach::EarlierNodeMay => {
            format!("unless a node before it writes the file, `{node}` stops there")
        }
        Reach::NothingEarlierCan => format!(
            "`{node}` stops there every time, since no node that runs before it can write the file"
        ),
    };
    let optional = "or declare the entry `optional: true` if the node can do without it";
    match (missing, base) {
        (M::Nowhere, Some(base)) => format!(
            "{reads}, which commit `{base}` — the one a run starts from — does not hold: \
             {stops}; commit the file first, {optional}"
        ),
        (M::Nowhere, None) => format!("{reads}, which does not exist: {stops}; {optional}"),
        (M::Uncommitted, _) => format!(
            "{reads}, which is in your checkout but not committed: a run starts from commit \
             `{}` and never sees it, so {stops}; commit it first",
            base.unwrap_or("HEAD")
        ),
        (M::Ignored, _) => format!(
            "{reads}, which git ignores: a run's tree never carries an ignored file, so \
             {stops}; add it with `git add -f`, or read a file git tracks"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{line, nodes};
    use yunta_core::caution::{Caution, MissingContextFile};
    use yunta_core::NodeId;

    #[test]
    fn the_warning_names_the_commit_the_run_starts_from_and_the_way_out() {
        let warning = Caution::ContextFileMissing {
            node: "plan".into(),
            path: "docs/architecture.md".to_string(),
            base: Some("36096d2c0a1b".to_string()),
            missing: MissingContextFile::Nowhere,
        };
        assert_eq!(
            line(&warning),
            "node `plan` reads `docs/architecture.md` (a `files:` context source), which commit \
             `36096d2c0a1b` — the one a run starts from — does not hold: unless a node before it \
             writes the file, `plan` stops there; commit the file first, or declare the entry \
             `optional: true` if the node can do without it"
        );
    }

    #[test]
    fn the_fan_out_list_writes_each_node_in_backticks_joined_by_commas() {
        let ids: Vec<NodeId> = vec!["a".into(), "b".into(), "c".into()];
        assert_eq!(nodes(&ids), "`a`, `b`, `c`");
    }

    #[test]
    fn a_push_to_the_base_branch_without_a_prior_gate_warns() {
        let push = Caution::PushToBaseWithoutGate {
            node: "pr".into(),
            branch: "main".to_string(),
        };
        assert_eq!(
            line(&push),
            "node `pr` pushes to the base branch (`main`) with no gate anywhere before it in the \
             DAG — put a gate ahead of the push, or push to `{{run.branch}}`"
        );
    }
}
