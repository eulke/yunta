//! See [`super`]. The receipt a finished run's scene shows, as its stage reads it: derived at
//! each draw from the ledgers of the run in view, as `yunta receipt` derives it from the log,
//! with the event chain its store verified.
//!
//! The chain is verified once for the run, as its receipt's scene first stands, read once as
//! every reading beyond the log is (`read_once`): never on a draw, on news of the run or as
//! the person moves away and back, and again only for another run the stage moves onto, or
//! where a stage opened over this one let the verification go before the store answered.
//! Verifying writes nothing, and no receipt is written beside the run. Until the store
//! answered, nothing of the receipt is said: its checks hold the chain's among them, and its
//! gist says the chain is intact. A verification that failed is said in the receipt's
//! place, in its own words, until the stage moves onto another run.

use yunta_core::receipt::{EventChainStatus, Receipt};

use super::scene::ComposedPane;
use super::{Framed, RunStage};
use crate::stage::Want;
use crate::{EvidenceOperation, Operation};

impl RunStage {
    /// The verification of the event chain of the run `framed`, asked where its scene stands
    /// as its receipt and the chain of that run was not asked for: once a run, and nothing
    /// anywhere else.
    pub(super) fn chain_asked(&mut self, framed: Option<&Framed<'_>>) -> Option<Want> {
        let framed = framed?;
        let pane = self.scene_of(framed)?.composes();
        if pane != ComposedPane::Receipt {
            return None;
        }
        let run = self.run.clone();
        self.chain
            .asked(run.clone())
            .then(|| Want::Work(Operation::Evidence(EvidenceOperation::Verify { run })))
    }

    /// What the store's answer verifying the run's event chain asks, `verified` or why it
    /// could not: nothing, the answer kept for the run it was asked for.
    pub(super) fn chain_read(&mut self, verified: Result<EventChainStatus, String>) -> Vec<Want> {
        self.chain.answered(verified);
        Vec::new()
    }

    /// The receipt of the run `framed`, derived from its ledgers and the chain its store
    /// verified, where the chain was verified for it; or why verifying it failed. None
    /// while the store has not answered, and for a run that has not closed, which has no
    /// receipt.
    pub(super) fn receipt_shown(&self, framed: &Framed<'_>) -> Option<Result<Receipt, String>> {
        let run = &framed.in_view.run;
        match self.chain.answer(&self.run)? {
            Ok(chain) => yunta_replay::receipt::closed_as_kept(
                run.run(),
                run.manifest(),
                run.state(),
                chain.clone(),
            )
            .ok()
            .map(Ok),
            Err(said) => Some(Err(said.clone())),
        }
    }

    /// The gist of the receipt of the run `framed`, where its scene shows it and its work
    /// holds: every criterion green, every finding answered and the log's chain intact.
    pub(super) fn receipt_gist(&self, framed: &Framed<'_>) -> Option<String> {
        let pane = self.scene_of(framed)?.composes();
        if pane != ComposedPane::Receipt {
            return None;
        }
        let receipt = self.receipt_shown(framed)?.ok()?;
        yunta_render::page::Receipt { receipt: &receipt }.gist()
    }
}
