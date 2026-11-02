//! How deep a page goes, as every command that draws one takes it.

use surco::doc::Depth;

/// How much of a page a reader asks for: a glance unless they ask for
/// more.
#[derive(Debug, Clone, Copy, clap::Args)]
pub(crate) struct DepthArg {
    /// Says what a glance leaves out: on a run's page every node with what
    /// it said, every task and every fact; on a run's stats its rails and
    /// each node's blocked share and models.
    #[arg(long)]
    full: bool,
    /// Says what `--full` says, and shows more: on a run's page its whole
    /// id, where it is kept, the session of each failed call and the files
    /// that hold whole outputs; on a run's stats each node's whole id.
    #[arg(long)]
    dig: bool,
}

impl DepthArg {
    /// The depth these flags ask for.
    pub(crate) fn depth(self) -> Depth {
        Depth::of(self.full, self.dig)
    }
}
