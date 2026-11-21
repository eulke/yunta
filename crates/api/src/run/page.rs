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
use yunta_core::ahead::NodesAhead;
use yunta_core::events::{ArtifactId, GateWaitingPayload, Shown};
use yunta_core::shown::{ShownContent, ShownDocument};
use yunta_core::{ArtifactKind, NodeId, NodeKind, TaskId};
use yunta_render::document::RunDocument;
use yunta_render::escalation::Decision;
use yunta_render::page::{Node, Page, Places, Run, RunScope};
use yunta_render::refusal::Refused;
use yunta_replay::frame::Moment;
use yunta_replay::RunState;

use super::{unread_keys, OpenedRun, RunReading};
use crate::{Document, Error, Failure, Remarks};

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

    /// The page of the run's node `node`: where it stands, what happened at it, what
    /// it hands over, each task's proof and scope where it is a loop, and the
    /// documents it produced. The run's events are folded once, each moment kept
    /// where it happened at `node`; each document is read on its own as a follower
    /// reads it, so one that fails today's shape is said unread in its place and the
    /// page still prints. A node the run does not declare is refused with the name it
    /// is nearest to.
    pub async fn node_page(self, node: &NodeId, remarks: &dyn Remarks) -> Result<Node, Error> {
        self.remark_unread_keys(remarks);
        let mut state = RunState::default();
        let moments: Vec<Moment> = self
            .events
            .iter()
            .filter_map(|event| state.chronicled(event))
            .filter(|moment| moment.node.as_ref() == Some(node))
            .collect();
        let frame = self.frame_of(&state, self.project.workspace.clock.now());
        let Some(framed) = frame.nodes.iter().find(|frame| frame.id == *node) else {
            return Err(Refused::NoSuchNode {
                run: self.id.clone(),
                node: node.clone(),
                declared: frame.nodes.iter().map(|frame| frame.id.clone()).collect(),
            }
            .into());
        };
        let workflow = &self.manifest.doc.workflow;
        let declares = workflow.iter_nodes().find(|declared| declared.id == *node);
        let declared = declares
            .and_then(|declared| declared.artifacts.as_ref())
            .map(|artifacts| artifacts.produces.clone())
            .unwrap_or_default();
        let tasks = match declares.map(|declared| &declared.kind) {
            Some(NodeKind::Loop { .. }) => self.titled_tasks(&state).await,
            _ => Vec::new(),
        };
        let (documents, unread) = self.produced(node, &state).await;
        let config = &self.manifest.doc.config;
        let runner = yunta_render::blocks::runner_of(framed, declares, &state, config).cloned();
        let workspace = &self.project.workspace;
        Ok(Node {
            run_id: self.id.clone(),
            node: framed.clone(),
            runner,
            run_dir: self.dir.clone(),
            cwd: workspace.dir.clone(),
            home: workspace.env.home.clone(),
            documents,
            unread,
            state,
            declared,
            tasks,
            moments,
        })
    }

    /// What `node` produced that its page reads whole, as the run in `state` holds
    /// it, each document read on its own: those read, a plan beside the spec it is
    /// judged with; and each of the node's own that cannot be read, with why.
    async fn produced(
        &self,
        node: &NodeId,
        state: &RunState,
    ) -> (Vec<ShownDocument>, Vec<(ArtifactId, String)>) {
        let asked = self.manifest.doc.request();
        let (mut read, mut unread) = (Vec::new(), Vec::new());
        for shown in yunta_engine::produced_shown(node, state) {
            match Document::read(&self.dir, &shown, state, asked).await {
                Document::Read(document) => read.push(document),
                Document::Unread { shown, why, .. } if shown.producer.as_ref() == Some(node) => {
                    unread.push((shown.artifact, why));
                }
                Document::Unread { .. } => {}
            }
        }
        (yunta_engine::beside_its_plan(read), unread)
    }

    /// Each task the run in `state` registered, in the order its latest tasks
    /// document lists them, with the title it gives each: the tasks a loop works. A
    /// task the document does not list follows in id order, untitled, as every task
    /// does where the document cannot be read.
    async fn titled_tasks(&self, state: &RunState) -> Vec<(TaskId, Option<String>)> {
        let plan = ArtifactId::Interpreted {
            kind: ArtifactKind::Tasks,
        };
        let shown = state.artifacts.latest(&plan, None).map(|held| Shown {
            producer: held.producer.clone(),
            artifact: held.artifact.clone(),
            content_hash: held.content_hash.clone(),
        });
        let asked = self.manifest.doc.request();
        let read = match &shown {
            Some(shown) => Some(Document::read(&self.dir, shown, state, asked).await),
            None => None,
        };
        let listed = match read {
            Some(Document::Read(ShownDocument {
                content: ShownContent::Tasks(review),
                ..
            })) => review.plan.tasks,
            _ => Vec::new(),
        };
        let mut tasks: Vec<(TaskId, Option<String>)> = listed
            .into_iter()
            .filter(|task| state.tasks.contains(&task.id))
            .map(|task| (task.id, Some(task.title)))
            .collect();
        for (task, _) in state.tasks.iter() {
            if !tasks.iter().any(|(listed, _)| listed == task) {
                tasks.push((task.clone(), None));
            }
        }
        tasks
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
        if let Some(remark) = unread_keys(&self.manifest) {
            remarks.remark(remark);
        }
    }
}

impl RunReading<'_> {
    /// The run's page as deep as `depth` asks, from this reading.
    async fn into_page(self, depth: Depth) -> Result<Run, Error> {
        let shown = self.shown().await?;
        let frozen = &self.run.manifest.doc;
        let ahead = NodesAhead::of(&frozen.workflow, &frozen.config, self.state.run.left_out());
        let decision = self.waits_on().map(|(on, escalation)| Decision {
            on: on.clone(),
            escalation: GateWaitingPayload::clone(escalation),
            shown,
            ahead,
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
