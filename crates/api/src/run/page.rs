//! See [`super`]. A run's page and a node's, read whole: everything render's
//! `page::Run` and `page::Node` are drawn from, read once at the project's clock's
//! instant, so `status`, `open` and an application's run stage show one page; the
//! run's document a program reads; and the run's page written where a browser
//! reads it.
//!
//! What a run's manifest holds that this binary does not read is said first, as a
//! remark: a reader acting on a manifest whose newer half is invisible to them
//! should know that is what they are doing.

use std::path::PathBuf;

use surco::doc::Depth;
use surco::surface::{Html, Surface};
use yunta_core::events::GateWaitingPayload;
use yunta_core::NodeId;
use yunta_render::document::RunDocument;
use yunta_render::escalation::Decision;
use yunta_render::page::{Node, Page, Places, Run, RunScope};
use yunta_render::refusal::Refused;

use super::{OpenedRun, RunReading};
use crate::{Error, Failure, Remark, Remarks};

/// The file a run's page is written to for a browser, in the run's folder.
const BROWSER_PAGE: &str = "page.html";

/// A run's page the disk would not take: where it went, and what the disk said.
#[derive(Debug, thiserror::Error)]
#[error("failed to write {}: {source}", path.display())]
struct PageUnwritten {
    path: PathBuf,
    source: std::io::Error,
}

impl OpenedRun<'_> {
    /// The run's page, as deep as `depth` asks: the decision it waits on with the
    /// documents that decision shows, and, at the depth that tells the run's story,
    /// the change each finished node's plan makes.
    pub async fn page(self, depth: Depth, remarks: &dyn Remarks) -> Result<Run, Error> {
        self.remark_unread_keys(remarks);
        self.reading().await?.into_page(depth).await
    }

    /// The page of the run's node `node`: where it stands, and the documents it
    /// produced. A node the run does not declare is refused with the name it is
    /// nearest to.
    pub async fn node_page(self, node: &NodeId, remarks: &dyn Remarks) -> Result<Node, Error> {
        self.remark_unread_keys(remarks);
        let reading = self.reading().await?;
        let Some(frame) = reading.frame.nodes.iter().find(|frame| frame.id == *node) else {
            return Err(Refused::NoSuchNode {
                run: reading.run.id.clone(),
                node: node.clone(),
                declared: reading
                    .frame
                    .nodes
                    .iter()
                    .map(|frame| frame.id.clone())
                    .collect(),
            }
            .into());
        };
        let workspace = &reading.run.project.workspace;
        Ok(Node {
            run_id: reading.run.id.clone(),
            node: frame.clone(),
            run_dir: reading.run.dir.clone(),
            cwd: workspace.dir.clone(),
            home: workspace.env.home.clone(),
            documents: reading.produced(node).await?,
            state: reading.state.clone(),
        })
    }

    /// The run's document, the one a program reads of where it stands.
    pub async fn document(self, remarks: &dyn Remarks) -> Result<RunDocument, Error> {
        self.remark_unread_keys(remarks);
        let reading = self.reading().await?;
        Ok(RunDocument::of(
            &reading.run.id,
            &reading.run.events,
            &reading.state,
            &reading.run.manifest.doc,
            &reading.frame,
            reading.decision.clone(),
        ))
    }

    /// Writes the run's page where a browser reads it, `page.html` in the run's
    /// folder, and hands back where: as deep as the whole story, with each document
    /// the decision it waits on is about whole before it, since a browser has the
    /// room a terminal does not.
    pub async fn page_for_a_browser(self, remarks: &dyn Remarks) -> Result<PathBuf, Error> {
        self.remark_unread_keys(remarks);
        let reading = self.reading().await?;
        let (path, now) = (reading.run.dir.join(BROWSER_PAGE), reading.now);
        let page = reading.into_page(Depth::Full).await?.in_a_browser(now);
        let drawn = Html.draw(&page.document(&Html.look()));
        tokio::fs::write(&path, drawn).await.map_err(|source| {
            Failure::new(PageUnwritten {
                path: path.clone(),
                source,
            })
        })?;
        Ok(path)
    }

    /// Says the keys of the run's manifest this binary does not read, when there
    /// are any.
    fn remark_unread_keys(&self, remarks: &dyn Remarks) {
        let keys = self.manifest.unknown_keys();
        if !keys.is_empty() {
            remarks.remark(Remark::UnreadManifestKeys {
                keys: keys.into_iter().map(str::to_string).collect(),
            });
        }
    }
}

impl RunReading<'_> {
    /// The run's page as deep as `depth` asks, from this reading.
    async fn into_page(self, depth: Depth) -> Result<Run, Error> {
        let shown = self.shown().await?;
        let decision = self.waits_on().map(|(on, escalation)| Decision {
            on: on.clone(),
            escalation: GateWaitingPayload::clone(escalation),
            shown,
        });
        let plans = match depth.reaches(Depth::Full) {
            true => self.plans().await,
            false => Vec::new(),
        };
        let manifest = self.run.manifest.doc;
        let workspace = &self.run.project.workspace;
        Ok(Run {
            scope: RunScope::Status,
            depth,
            run_id: self.run.id,
            frame: self.frame,
            state: self.state,
            places: Places {
                run_dir: self.run.dir,
                worktree: self.tree,
                branch: self.branch,
                base_branch: manifest.base_branch,
                isolation: manifest.isolation,
                cwd: workspace.dir.clone(),
                home: workspace.env.home.clone(),
            },
            decision,
            plans,
        })
    }
}
