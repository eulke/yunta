//! Giving a checkout back to the pool: its use recorded, the build it
//! leaves recorded, and its hold let go.

use std::path::Path;

use super::{space, CheckoutPool, Lease};
use crate::process::Supervision;
use crate::worktree::WorktreeError;

impl CheckoutPool {
    /// Gives back the checkout `lease` holds, its user done with it: its
    /// record says it was used until now, what its build leaves is recorded
    /// while the hold still stands — and counted out of the caller's way,
    /// so the next user, who takes a checkout holding a build first, knows —
    /// and only then the hold goes.
    pub async fn give_back(&self, lease: Lease, supervision: Supervision<'_>) {
        let (home, number) = (lease.home.clone(), lease.number);
        let now = supervision.clock.now();
        let checkout = lease.checkout();
        let roots = space::ignored(&checkout, supervision).await;
        super::slot::used(&home, number, self.holder.as_deref(), now).await;
        if let Some(roots) = roots {
            space::given_back(&home, number, roots, now, &self.walking);
        }
        drop(lease);
    }

    /// Puts the checkout `lease` holds back to the commit it is on, with
    /// nothing a probe wrote left in it but what git ignores, and gives it
    /// back. One that cannot be put back stays out of the pool.
    pub async fn put_back(&self, lease: Lease, supervision: Supervision<'_>) {
        let checkout = lease.checkout();
        match clear_work(&checkout, supervision).await {
            Ok(()) => self.give_back(lease, supervision).await,
            Err(error) => tracing::debug!(%error, "a probe's checkout stays out of the pool"),
        }
    }
}

/// Clears what a probe wrote in `checkout`: back to the commit it is on,
/// with nothing left in it but what git ignores.
pub async fn clear_work(
    checkout: &Path,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    crate::git::output(checkout, &["reset", "-q", "--hard"], supervision).await?;
    crate::git::output(checkout, &["clean", "-q", "-ffd"], supervision).await?;
    Ok(())
}
