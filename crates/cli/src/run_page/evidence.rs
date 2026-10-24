//! The evidence of what failed: a section to each failed node.

use surco::components::{Said, Section, Whole};
use surco::doc::{Block, Depth};
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use yunta_core::events::Failure;
use yunta_engine::NodeFrame;
use yunta_render::blocks::FailureDetail;

use super::{failed, RunPage};
use crate::commands::advice;

impl RunPage<'_> {
    /// Every failed node, under its id, with what its failure says: its
    /// claim beside the id — unless the page's gist made it word for word —
    /// then the end of what a command printed and where the rest is, each
    /// document it refused, the paths it should not have touched. A node
    /// whose failure has nothing more to quote, and whose claim the gist
    /// made, is not listed again.
    pub(super) fn evidence(&self, glyphs: Glyphs) -> Vec<Block> {
        let asked = self.gist().unwrap_or_default();
        failed(self.frame)
            .filter_map(|(node, failure)| {
                let claim = yunta_render::failure::headline(failure);
                let said = asked.contains(&format!("`{}` failed: {claim}", node.id));
                let mut title = Line::new().push(Tone::Strong, node.id.as_str());
                let mut blocks: Vec<Block> = Vec::new();
                match (said, claim.contains('\n')) {
                    (true, _) => {}
                    (false, false) => {
                        title = title.push(Tone::Muted, format!(" {} {claim}", glyphs.sep()));
                    }
                    // A claim that lists what stopped the node keeps its lines.
                    (false, true) => {
                        blocks.push(Said(claim.lines().map(str::to_string).collect()).into());
                    }
                }
                let whole = self.whole(node, failure);
                blocks.extend(FailureDetail { failure, whole }.blocks());
                (!said || !blocks.is_empty()).then(|| {
                    Section {
                        lead: Some(surco::components::Lead::Mark(Mark::Failed)),
                        title,
                        blocks,
                    }
                    .into()
                })
            })
            .collect()
    }

    /// Where the whole of what a node's command printed is: the command
    /// that shows it, and — for a reader who digs — the file it is kept in.
    fn whole(&self, node: &NodeFrame, failure: &Failure) -> Option<Whole> {
        let output = failure.output()?;
        Some(match self.depth.reaches(Depth::Dig) {
            true => Whole::file(
                yunta_engine::ObjectStore::at(self.paths.run_dir).path_of(output),
                self.paths.cwd,
                self.paths.home,
            ),
            false => Whole::Command(advice::status_node(self.run_id.handle(), node.id.as_str())),
        })
    }
}
