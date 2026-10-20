//! How deep a page goes, as every command that draws one takes it.

use surco::doc::Depth;

/// How much of a page a reader asks for: a glance unless they ask for
/// more.
#[derive(Debug, Clone, Copy, clap::Args)]
pub(crate) struct DepthArg {
    /// Tells the whole story: every node with what it said, how long it
    /// worked and what it spent, every task, and every fact a glance
    /// leaves out.
    #[arg(long)]
    full: bool,
    /// Tells the whole story with every trace the run keeps: its whole
    /// id, where it is kept, and the files that hold whole outputs.
    #[arg(long)]
    dig: bool,
}

impl DepthArg {
    /// The depth these flags ask for.
    pub(crate) fn depth(self) -> Depth {
        Depth::of(self.full, self.dig)
    }
}
