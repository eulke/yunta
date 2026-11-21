//! The successor a promotion asks for, created for a run a [`Bench`] drove to
//! its close as `promoted`, as the drive that closed it creates one: born from
//! the run's checkout, its id minted by the bench's ids, and never run here.

use yunta_core::{ModeName, RunId};

use super::{no_secrets, Bench};

impl Bench {
    /// Creates the successor in `mode` that this bench's run, closed
    /// `promoted`, asks for, and answers its id. The successor is created and
    /// left unwoken, as a drive that stops after the promotion leaves it.
    pub async fn successor(&self, mode: &str) -> RunId {
        let manifest = self.manifest();
        let run_dir = self.run_dir();
        let tree = self.tree();
        let worktrees = self
            .runs_root
            .parent()
            .map(|parent| parent.join("worktrees"))
            .unwrap_or_else(|| self.runs_root.join("worktrees"));
        let secrets = no_secrets();
        let successor = yunta_engine::create_promotion_successor(
            yunta_engine::Predecessor {
                id: &self.run_id,
                manifest: &manifest,
                worktree: &tree,
                run_dir: &run_dir,
            },
            &ModeName::from(mode),
            yunta_engine::RunRoots {
                runs: &self.runs_root,
                worktrees: &worktrees,
            },
            yunta_engine::CallerInfra {
                storage: &self.storage.async_handle(),
                ids: self.ids.as_ref(),
                supervision: self.supervision(),
                environment: yunta_engine::process::execution_environment(self.ambient.as_ref()),
                secrets: secrets.as_ref(),
            },
        )
        .await
        .expect("the successor the promotion asks for is created");
        successor.run_id
    }
}
