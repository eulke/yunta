//! The writes the fence refused, as a scope's card rests on them: each path the request asks
//! for that a session of the node tried to write and the fence turned away, once, oldest
//! first, under the failed mark, the path in the tone of what a reader copies. A request
//! that no refused write led to says nothing here: the card asks it whole.
//!
//! Which paths a request asks for is the escalation's: a task's request asked at the
//! engine's terminal names them in its menu, and a loop that ended owing a person its tasks'
//! requests names them in its failure. A write is the fence's to have refused only where the
//! node's ledger keeps it ([`RefusedWrite`]), so nothing here says a refusal the log does not
//! hold.

use surco::doc::Block;
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use yunta_core::events::{Failure, GateWaitingPayload, Menu, RefusedWrite};
use yunta_core::ScopeGlob;

/// The words before the path of a write the fence refused.
const REFUSED: &str = "the fence refused a write";

/// The writes the fence refused that a scope request asks to make, as a scope's card rests
/// on them. Not [`super::TaskScope`], the paths a task may write and those it wrote.
pub struct FenceRefused {
    /// Each path refused, once, in the order the log first refused it.
    paths: Vec<String>,
}

impl FenceRefused {
    /// The writes among `refused`, those a node's sessions had refused as its ledger keeps
    /// them, whose path the scope request `escalation` asks for: each path once, oldest
    /// first. None where the escalation asks for no path.
    pub fn asked_by(escalation: &GateWaitingPayload, refused: &[RefusedWrite]) -> FenceRefused {
        let asked = asked_for(escalation);
        let mut paths: Vec<String> = Vec::new();
        let named = refused.iter().filter_map(|write| write.target.display());
        for path in named {
            let wanted = asked
                .iter()
                .any(|glob| glob.compiled().compile_matcher().is_match(path));
            if wanted && !paths.iter().any(|seen| seen == path) {
                paths.push(path.to_string());
            }
        }
        FenceRefused { paths }
    }

    /// Its blocks, in `glyphs`: a row each path the fence refused, under the failed mark;
    /// none where it refused none.
    pub fn blocks(&self, glyphs: Glyphs) -> Vec<Block> {
        if self.paths.is_empty() {
            return Vec::new();
        }
        let mark = format!("{} ", glyphs.mark(Mark::Failed));
        let rows = self.paths.iter().map(|path| {
            Line::new()
                .push(Tone::of(Mark::Failed), mark.as_str())
                .plain(format!("{REFUSED}: "))
                .push(Tone::Command, path.as_str())
        });
        vec![Block::Rows(rows.collect())]
    }
}

/// The paths the scope request `escalation` asks for: a task's request asked at the engine's
/// terminal, or each request a loop that failed owing them asks. None for any other
/// escalation.
fn asked_for(escalation: &GateWaitingPayload) -> Vec<&ScopeGlob> {
    match escalation.menu() {
        Some(Menu::Scope(wait)) => wait.paths.iter().collect(),
        Some(Menu::Failure(menu)) => match &menu.failure {
            Failure::ScopeOwed { owed } => owed.iter().flat_map(|asked| &asked.paths).collect(),
            _ => Vec::new(),
        },
        Some(
            Menu::Gate(_)
            | Menu::Console(_)
            | Menu::Published(_)
            | Menu::Budget(_)
            | Menu::LoopCap(_)
            | Menu::Departure(_),
        )
        | None => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
