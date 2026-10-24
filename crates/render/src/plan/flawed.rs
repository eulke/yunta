//! What keeps a plan from being proven as it is written, each flaw as a
//! caution: what is so, what it means, and what fixes it.

use surco::components::Concern;
use surco::doc::Block;
use yunta_core::shown::Flaw;

/// Each of `flaws` as a caution.
pub(crate) fn concerns(flaws: &[Flaw]) -> Vec<Block> {
    flaws
        .iter()
        .map(|flaw| {
            Block::Concern(Concern {
                fact: flaw.to_string(),
                so: flaw.so(),
                fix: surco::components::Fix::said(flaw.fix()),
            })
        })
        .collect()
}
