//! Which skill directories a node's sessions mount: the names the node and
//! the config ask for, each read from what the run froze.

use std::path::PathBuf;

use yunta_core::events::Failure;
use yunta_core::port::Adapter;
use yunta_core::Node;

use crate::frozen::{FrozenError, Source};
use crate::run::node_exec::NodeEnd;

/// The skill directories this node's sessions mount. The names are the
/// node's own plus the config's `skills.always`, and each one resolves
/// through the run's frozen set, so a skill edited mid-run mounts as it was
/// at birth. A name the run did not freeze, or whose frozen file no longer
/// matches its hash, fails the node; mounting is the adapter's, and an
/// adapter without the capability degrades with an event rather than
/// failing — a skill is added instruction, never correctness.
pub(super) async fn mount_skills(
    ctx: &crate::run::RunCtx<'_>,
    node: &Node,
    adapter: &dyn Adapter,
) -> Result<Result<Vec<PathBuf>, NodeEnd>, crate::run::RunError> {
    let source = Source::of(ctx.manifest, ctx.run_dir, ctx.worktree, ctx.ambient);
    let mut skills = Vec::new();
    for name in crate::skills::skill_names(&ctx.manifest.config, &ctx.manifest.workflow, node) {
        match source.skill_dir(&name).await {
            Ok(dir) => skills.push(dir),
            // A name that resolves to no directory is a skill the run does not hold.
            Err(FrozenError::Unresolved(_)) => {
                let searched = searched_roots(ctx, &name);
                let failure = Failure::SkillMissing {
                    missing_skill: name,
                    searched,
                };
                return Ok(Err(crate::run::node_close::fail(ctx, node, failure).await?));
            }
            Err(error) => {
                return Ok(Err(crate::run::session_policy::close_frozen(
                    ctx, node, error,
                )
                .await?))
            }
        }
    }
    // A node that mounts none asked for nothing, so nothing is missing.
    if skills.is_empty() {
        return Ok(Ok(skills));
    }
    Ok(Ok(
        match crate::run::capability::require(ctx, adapter, yunta_core::Capability::Skills, node)
            .await?
        {
            crate::run::capability::Decision::Granted => skills,
            crate::run::capability::Decision::Degraded => Vec::new(),
            crate::run::capability::Decision::Refused(error) => return Err(error),
        },
    ))
}

/// The directories a skill name that resolves to nothing was searched under.
/// The checkout is the only source whose lookup fails this way, and it
/// searches the roots its config names, the same way again.
fn searched_roots(ctx: &crate::run::RunCtx<'_>, name: &yunta_core::SkillName) -> Vec<PathBuf> {
    match crate::skills::locate(&ctx.manifest.config, ctx.worktree, name) {
        Err(crate::skills::SkillsError::NotFound { searched, .. }) => searched,
        Ok(_) => Vec::new(),
    }
}
