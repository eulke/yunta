//! Whether a session may end its turn: its work judged the way the
//! attempt's close will judge it, while the session can still act on the
//! answer.
//!
//! A CLI asks before its session stops. A task session's work is checked
//! as `yunta_check_task` checks it — criteria, guards, scope — and a
//! node's own session's as its close checks it: the documents it hands
//! over and its scope. When the close would not take that work, the
//! session is sent back to it with what stands in the way, and the log
//! says so before the session hears it.
//!
//! Never twice for one state of the work: a session that stops again
//! without changing anything has nothing better to give, and holding it
//! would only spend the run. Never while the attempt's end owes the
//! session an answer about its scope — a request it made, or a refused
//! write a red criterion locates — because that end decides it and
//! resumes the session with the answer. And never once the session is
//! being cut: a stop then is the end it was asked for.

use yunta_core::diagnostic::{ArtifactFailure, FileProblem};
use yunta_core::events::{EventPayload, SessionEvent, StopRefusal, StopRefusedPayload};
use yunta_core::rail::RailAnswer;
use yunta_core::{ArtifactSpec, ContentHash, RunTool, SessionId};

use super::host::TaskAccess;
use super::rail::refused_writes;
use super::session::{RunToolError, SessionTools};
use crate::artifacts::{answerer, Answerer, RunArtifacts};
use crate::process::Printed;
use crate::task_cycle::CriterionRun;

/// Work a session would leave that its close would not take.
struct Unfinished {
    /// What the work stands at, which a stop is refused once for.
    state: ContentHash,
    why: Vec<StopRefusal>,
    /// What the session reads: what stands in the way, and how to go on.
    told: String,
    /// What each red criterion printed, read for a refused write it
    /// locates.
    red: Vec<String>,
}

impl SessionTools {
    /// Whether the session may stop now.
    pub(super) async fn before_stop(&self, session: &SessionId) -> RailAnswer {
        let judged = match self.task.as_deref() {
            Some(access) => self.task_unfinished(access).await,
            None => self.node_unfinished().await,
        };
        let unfinished = match judged {
            Ok(Some(unfinished)) => unfinished,
            Ok(None) => return RailAnswer::Proceed,
            Err(error) => {
                tracing::debug!(%error, "a stop went unjudged");
                return RailAnswer::Proceed;
            }
        };
        if self.stop.is_cancelled() {
            return RailAnswer::Proceed;
        }
        match self.scope_owed(session, &unfinished).await {
            Ok(false) => {}
            Ok(true) => return RailAnswer::Proceed,
            Err(error) => {
                tracing::debug!(%error, "a stop's scope owed went unread");
                return RailAnswer::Proceed;
            }
        }
        match self.refused_before(session, &unfinished.state).await {
            Ok(false) => {}
            Ok(true) => return RailAnswer::Proceed,
            Err(error) => {
                tracing::debug!(%error, "a stop's earlier refusals went unread");
                return RailAnswer::Proceed;
            }
        }
        let payload = EventPayload::Session(SessionEvent::StopRefused(StopRefusedPayload {
            session_id: session.clone(),
            state: unfinished.state,
            why: unfinished.why,
            task_id: self.task.as_ref().map(|access| access.task.id.clone()),
        }));
        // A refusal the log does not hold could be repeated forever: the
        // session stops instead.
        if let Err(error) = self.append(payload).await {
            tracing::debug!(%error, "a refused stop went unrecorded");
            return RailAnswer::Proceed;
        }
        RailAnswer::Refuse {
            reason: unfinished.told,
        }
    }

    /// A task session's work, checked as `yunta_check_task` checks it —
    /// `None` when it closes its task, and when a criterion could not run
    /// at all, which nothing the session does changes.
    async fn task_unfinished(
        &self,
        access: &TaskAccess,
    ) -> Result<Option<Unfinished>, RunToolError> {
        let (judgement, tree) = self.judged_now(access).await?;
        let unrunnable = judgement
            .criteria
            .iter()
            .any(|run| run.could_not_run().is_some());
        if judgement.closes() || unrunnable {
            return Ok(None);
        }
        let mut why = Vec::new();
        if !judgement.waiting.is_empty() || judgement.criteria.iter().any(|run| run.exit_code != 0)
        {
            why.push(StopRefusal::CriteriaRed);
        }
        if !judgement.scope.violations.is_empty() {
            why.push(StopRefusal::OutsideScope);
        }
        let red = self.red_output(&judgement.criteria).await;
        let verdict = self.check_verdict(judgement)?;
        let told = format!(
            "yunta: not done yet — this is your work judged as the engine will judge it when \
             your session ends, and it would not close your task:\n{verdict}\n\nFix what fails, \
             then finish. If the fix needs a file outside your scope, ask for it with `{}` and \
             finish; the engine answers it once your session ends. Finishing again without \
             changing anything ends the session as it stands.",
            self.called(RunTool::RequestScopeExpansion)
        );
        Ok(Some(Unfinished {
            state: ContentHash::sha256(tree.as_str().as_bytes()),
            why,
            told,
            red,
        }))
    }

    /// A node's own session's work, checked as its close checks it: every
    /// document the session hands over, and its scope. `None` when the
    /// close would take it.
    async fn node_unfinished(&self) -> Result<Option<Unfinished>, RunToolError> {
        let (mut why, mut told) = self.documents_owed().await?;
        if let Some(access) = self.node_scope.as_deref() {
            let audit = self.scope_audit(access).await?;
            if !audit.violations.is_empty() {
                why.push(StopRefusal::OutsideScope);
                let paths: Vec<String> = audit
                    .violations
                    .iter()
                    .map(|path| format!("- {}", path.display()))
                    .collect();
                told.push(format!(
                    "outside this node's scope — undo these changes:\n{}",
                    paths.join("\n")
                ));
            }
        }
        if why.is_empty() {
            return Ok(None);
        }
        let tree = crate::task_cycle::content_of(&self.cwd, self.host.supervision(&self.stop))
            .await
            .map_err(|source| RunToolError::Check { source })?;
        let found = told.join("\n\n");
        let state = ContentHash::sha256(format!("{tree}\n{found}").as_bytes());
        let told = format!(
            "yunta: not done yet — this is your work judged as this node's close will judge \
             it, and it would not take it:\n\n{found}\n\nFix what it names, then finish. \
             Finishing again without changing anything ends the session as it stands."
        );
        Ok(Some(Unfinished {
            state,
            why,
            told,
            red: Vec::new(),
        }))
    }

    /// What keeps the documents this session hands over from closing its
    /// node, and each one's verdict. A document the engine derives or
    /// writes itself — findings from what was posted, answers a person
    /// gave — is the close's to take, not the session's to hand over.
    async fn documents_owed(&self) -> Result<(Vec<StopRefusal>, Vec<String>), RunToolError> {
        let handed: Vec<&ArtifactSpec> = self
            .declared
            .iter()
            .filter(|spec| match answerer(&self.node_kind, spec.kind()) {
                Answerer::Staging => true,
                Answerer::Log => spec.kind().is_some_and(|kind| kind.submit_tool().is_some()),
            })
            .collect();
        let mut why = Vec::new();
        let mut told = Vec::new();
        if handed.is_empty() {
            return Ok((why, told));
        }
        let events = self.events().await?;
        let held = RunArtifacts::of(&self.host.run_dir, &events);
        for spec in handed {
            let verified = self.verified(spec, &held).await;
            let Err(failure) = &verified else {
                continue;
            };
            let reason = match missing(failure) {
                true => StopRefusal::ArtifactMissing,
                false => StopRefusal::ArtifactRefused,
            };
            if !why.contains(&reason) {
                why.push(reason);
            }
            told.push(super::submission::render_verdict(
                &spec.to_string(),
                verified,
            ));
        }
        Ok((why, told))
    }

    /// Whether the attempt's end owes this session an answer about its
    /// scope: a request it made, or a write it was refused that a red
    /// criterion locates. The log is read only when a red criterion has
    /// output to locate a write in.
    async fn scope_owed(
        &self,
        session: &SessionId,
        unfinished: &Unfinished,
    ) -> Result<bool, RunToolError> {
        let request = self
            .cwd
            .join(crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE);
        if tokio::fs::try_exists(&request).await.unwrap_or(false) {
            return Ok(true);
        }
        if unfinished.red.is_empty() {
            return Ok(false);
        }
        let refused = refused_writes(&self.events().await?, std::slice::from_ref(session));
        Ok(!refused.is_empty()
            && unfinished
                .red
                .iter()
                .any(|printed| crate::task_cycle::locates_a_refusal(printed, &self.cwd, &refused)))
    }

    /// Whether `session` was sent back already with its work at `state`.
    async fn refused_before(
        &self,
        session: &SessionId,
        state: &ContentHash,
    ) -> Result<bool, RunToolError> {
        let derived = crate::replay::derive(&self.events().await?);
        Ok(derived.nodes.stop_refused_on(&self.node, session, state))
    }

    /// Everything each red criterion of `runs` printed: what this check
    /// ran, or what the run kept of an answer it reused.
    async fn red_output(&self, runs: &[CriterionRun]) -> Vec<String> {
        let objects = crate::artifacts::store::ObjectStore::at(&self.host.run_dir);
        let mut texts = Vec::new();
        for run in runs.iter().filter(|run| run.exit_code != 0) {
            let text = match &run.output {
                Some(Printed::Ran(output)) => String::from_utf8_lossy(output.bytes()).into_owned(),
                Some(Printed::Recorded { object, tail }) => {
                    let kept = match object {
                        Some(hash) => objects.get(hash).await.ok(),
                        None => None,
                    };
                    match kept {
                        Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                        None => tail.join("\n"),
                    }
                }
                None => continue,
            };
            texts.push(text);
        }
        texts
    }
}

/// Whether `failure` is a document never handed over, rather than one
/// handed over that the close refuses.
fn missing(failure: &ArtifactFailure) -> bool {
    matches!(
        failure,
        ArtifactFailure::Undelivered { .. }
            | ArtifactFailure::File {
                problem: FileProblem::Missing { .. },
                ..
            }
    )
}
