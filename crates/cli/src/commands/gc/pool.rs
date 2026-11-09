//! What a pass of `gc` does to the project's pool: it takes in the units'
//! branches runs made before refs kept their work, and it tidies the
//! checkouts' builds within their budget, saying what the pool keeps.

use std::path::Path;

use yunta_render::page::Collected;

use super::problem;
use crate::context::Context;

/// Takes in the units' branches runs made before refs kept their work:
/// what each unit committed stays, kept by its ref, and no list of the
/// project's branches shows it.
pub(super) async fn adopt(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    collected: &mut Collected,
) {
    match yunta_engine::adopt_unit_branches(pool, &ctx.cwd, None, ctx.supervision()).await {
        Ok(taken) if !taken.is_empty() => {
            let branches = surco::text::counted_as(taken.len(), "unit branch", "unit branches");
            collected.did("kept", format!("the work of {branches} by refs"));
        }
        Ok(_) => {}
        Err(e) => problem(collected, format!("the units' branches: {e}")),
    }
}

/// Says which of the units' branches runs made before refs kept their work
/// a pass would take in, taking none.
pub(super) async fn would_adopt(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    collected: &mut Collected,
) {
    match yunta_engine::adoptable_unit_branches(pool, &ctx.cwd, ctx.supervision()).await {
        Ok(branches) if !branches.is_empty() => {
            let branches = surco::text::counted_as(branches.len(), "unit branch", "unit branches");
            collected.did("would keep", format!("the work of {branches} by refs"));
        }
        Ok(_) => {}
        Err(e) => problem(collected, format!("the units' branches: {e}")),
    }
}

/// Tidies the project's pool — what its checkouts' builds take is
/// measured where nobody did, a build unused within retention is emptied,
/// the builds stay within the budget, and a free checkout holding no
/// build goes — and says what the pool keeps.
pub(super) async fn tidy(
    ctx: &Context,
    pool: &yunta_engine::CheckoutPool,
    collected: &mut Collected,
) {
    let unused = ctx
        .project
        .config
        .storage
        .as_ref()
        .and_then(|storage| storage.retention_days)
        .map(|days| chrono::Duration::days(i64::from(days)));
    let shown = |dir: &Path| surco::paths::shown(dir, &ctx.cwd, ctx.env.home.as_deref());
    match pool.tidy(unused, ctx.supervision()).await {
        Ok(tidied) => {
            for (checkout, bytes) in &tidied.emptied {
                collected.did(
                    "emptied",
                    format!(
                        "the build of {} ({})",
                        shown(checkout),
                        surco::figures::Bytes(*bytes)
                    ),
                );
            }
            for checkout in &tidied.removed {
                collected.did(
                    "removed",
                    format!("the unused checkout {}", shown(checkout)),
                );
            }
            collected.now = Some(tidied.kept.to_string());
        }
        Err(e) => problem(collected, format!("the project's checkouts: {e}")),
    }
}
