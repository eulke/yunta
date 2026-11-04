//! What a pack command says: the headline over the inventory a person reads
//! before a pack is vendored, and the page an install answers with.

use std::path::Path;

use surco::components::{Fields, Furrow, Headline};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::text::counted;
use surco::words::Standing;
use yunta_core::{CommitSha, PackExecutorPolicy, PackManifest};

use super::super::pack_audit::PackTestSummary;
use super::PackPolicy;

/// The headline of the inventory a person reads before a pack is
/// vendored: it asks for them when the executors the pack declares wait on
/// their `--yes`, and says the pack is `doing` it otherwise.
pub(super) fn about_to(
    policy: &PackPolicy,
    manifest: &PackManifest,
    confirmed: bool,
    doing: &str,
) -> Headline {
    let pack = format!("pack {}", manifest.reference());
    let executors = manifest.declares.executors.len();
    let asks = executors > 0 && policy.executors == PackExecutorPolicy::Prompt && !confirmed;
    let headline = match asks {
        true => Headline::new(Mark::NeedsYou, pack, Standing::NeedsYou.to_string()),
        false => Headline::new(Mark::Running, pack, doing),
    }
    .fact(manifest.version.to_string());
    match executors {
        0 => headline,
        executors => headline.fact(counted(executors, "executor")),
    }
}

/// What an install answers with: the pack, its version and commit, where it
/// is, what its own cases say, and what it needs from this project.
pub(super) struct Installed<'a> {
    pub(super) manifest: &'a PackManifest,
    pub(super) commit: &'a CommitSha,
    pub(super) at: &'a Path,
    pub(super) cwd: &'a Path,
    pub(super) tests: &'a PackTestSummary,
    pub(super) needs: Vec<Block>,
}

impl Installed<'_> {
    /// The page: installed, unless one of the pack's own cases failed,
    /// which is worth a look.
    pub(super) fn page(self) -> Doc {
        let pack = format!("pack {}", self.manifest.reference());
        let headline = Headline::new(self.tests.mark(), pack, "installed")
            .fact(self.manifest.version.to_string())
            .fact(self.commit.abbreviated());
        let at = surco::paths::shown(self.at, self.cwd, None);
        Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with(Fields::new().push_if("at", at))
            .with_all(self.tests.blocks())
            .with_all(self.needs)
    }
}
