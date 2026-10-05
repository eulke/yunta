//! Handing a whole document to the engine, and asking what the close
//! will make of it.
//!
//! A typed artifact is never a file the session writes: it is submitted
//! as an object, validated by the code the node's close runs, and taken
//! into the run once it is accepted — bytes in the object store, an
//! `artifact_accepted` on the log. The verdict a session asks for reads
//! that same document back out of the run, exactly as the close does — a
//! verdict it can act on while it still can has to be the verdict that
//! decides the node, or the first one teaches false confidence.
//!
//! What a session may submit is bounded by what its node declares: a
//! tool exists only for a kind that node produces, so a document the
//! close would never look for has no way in and there is nothing for the
//! session to choose.

use serde_json::Value;
use yunta_core::diagnostic::{ArtifactFailure, DocumentRef, Report};
use yunta_core::events::{
    ArtifactId, ArtifactSubmittedPayload, EventPayload, RecordedOrigin, SubmissionOutcome,
    TaskProbe,
};
use yunta_core::{ArtifactKind, ArtifactSpec};

use crate::artifacts::{accept, VerifiedArtifact};

use super::catalog::RunTool;
use super::session::{RunToolError, SessionTools};
use super::verdicts::{failure_heading, numbered, read_as, submission_refusal, Reply};
use yunta_core::events::ArtifactEvent;

impl SessionTools {
    /// The verdict this node's close will reach, while the session can
    /// still act on it.
    ///
    /// What comes back on success is what the engine understood, not
    /// just that the document parsed: a tasks document that reads as six
    /// tasks when the session meant seven is a failure nothing else
    /// catches.
    pub(super) async fn check_artifact(
        &self,
        args: &serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let wanted = args.get("name").and_then(Value::as_str);
        let specs: Vec<ArtifactSpec> = self
            .declared
            .iter()
            .filter(|spec| wanted.is_none_or(|name| spec.to_string() == name))
            .cloned()
            .collect();
        if specs.is_empty() {
            return Err(self.nothing_to_check(wanted));
        }
        let events = self.events().await?;
        let held = crate::artifacts::RunArtifacts::of(&self.host.run_dir, &events);
        let mut verdicts: Vec<String> = Vec::with_capacity(specs.len());
        for spec in &specs {
            verdicts.push(self.verdict(spec, &held).await);
        }
        Ok(verdicts.join("\n\n"))
    }

    /// What this node's close would say about one declared artifact right
    /// now.
    ///
    /// Run tools belong to a session, so this node is one whose typed
    /// artifacts are never files it writes: a document it declares under
    /// a kind is answered by what the run holds, and an opaque artifact
    /// is the file the session writes itself. Those are the two answers
    /// the close reaches, through these same two functions — which is
    /// what keeps the verdict a session can still act on and the verdict
    /// that decides the node one answer.
    async fn verdict(
        &self,
        spec: &ArtifactSpec,
        held: &crate::artifacts::RunArtifacts<'_>,
    ) -> String {
        let verified = match crate::artifacts::answerer(&self.node_kind, spec.kind()) {
            crate::artifacts::Answerer::Log => {
                crate::artifacts::held_document(&self.node, spec, held).await
            }
            crate::artifacts::Answerer::Staging => {
                crate::artifacts::verify_one(
                    &self.node,
                    spec,
                    &self.host.run_dir,
                    self.host.max_artifact_bytes,
                )
                .await
            }
        };
        render_verdict(&spec.to_string(), verified)
    }

    /// Why a check has nothing to look at: a name this node does not
    /// declare, or a node that declares nothing at all.
    fn nothing_to_check(&self, wanted: Option<&str>) -> RunToolError {
        match wanted {
            Some(name) => RunToolError::UndeclaredArtifact {
                name: name.to_string(),
                declared: self.declared_names(),
            },
            None => RunToolError::NoArtifacts {
                node: self.node.clone(),
            },
        }
    }

    /// Takes one document a session submitted: validates it against the
    /// kind the node declared and, once accepted, takes it into the run
    /// as the artifact the close asks the log for.
    pub(super) async fn submit(
        &self,
        kind: ArtifactKind,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        // The node declared the kind and this tool is that kind's: there
        // is nothing for the call to name, so the only thing it carries
        // is the document.
        let spec = ArtifactSpec::Interpreted(kind);
        if !self.declared.contains(&spec) {
            return Err(RunToolError::UndeclaredArtifact {
                name: kind.to_string(),
                declared: self.declared_names(),
            });
        }
        let Some(document) = args.get("document").filter(|value| value.is_object()) else {
            return Err(self.invalid_submission(kind, "`document` is missing or is not an object"));
        };
        let offered = crate::artifacts::submit(
            &self.node,
            &spec,
            document.clone(),
            self.host.max_artifact_bytes,
        );
        let offered = match offered {
            Ok(verified) => self.proven(kind, verified).await?,
            refused => Offered::told_nothing(refused),
        };
        self.record(kind, offered).await
    }

    /// A readable document, held to what only running its commands can
    /// settle: a tasks document's criteria, or a spec's tests, run where
    /// the engine runs them. Refused with every rule they break, exactly
    /// as a document that broke its shape is.
    async fn proven(
        &self,
        kind: ArtifactKind,
        verified: VerifiedArtifact,
    ) -> Result<Offered, RunToolError> {
        let (broken, told, probes) = match &verified.content {
            crate::artifacts::ArtifactContent::Tasks(tasks) => {
                let (broken, probes) = self.handover(tasks).await?;
                (broken, String::new(), probes)
            }
            crate::artifacts::ArtifactContent::Spec(spec) => {
                let proven = self.spec_handover(spec).await?;
                let told = failing_now(&proven.failing, &self.host.redactor);
                (proven.broken, told, by_task(&proven.failing))
            }
            _ => return Ok(Offered::told_nothing(Ok(verified))),
        };
        if broken.is_empty() {
            return Ok(Offered {
                verdict: Ok(verified),
                told,
                probes,
            });
        }
        Ok(Offered {
            verdict: Err(crate::artifacts::SubmitError::Refused(Report::new(
                DocumentRef::new(kind, verified.path.display().to_string()),
                broken,
            ))),
            told: String::new(),
            probes,
        })
    }

    /// Records the engine's verdict on a submitted document and answers
    /// the session with it: what was read out of the document on
    /// acceptance, what to fix on a refusal. Both are events — a run
    /// reads back every document a session offered, not only the ones
    /// that landed.
    ///
    /// Two facts, not one: `artifact_submitted` is the call this session
    /// made and how it was answered, and it is on the log whether the
    /// document landed or not. An accepted document is also an artifact
    /// the run now holds, which is what [`accept`] states — so a reader
    /// asking what the run holds never has to know that a session is
    /// what handed it over.
    async fn record(&self, kind: ArtifactKind, offered: Offered) -> Result<String, RunToolError> {
        let Offered {
            verdict: offered,
            told,
            probes,
        } = offered;
        let name = ArtifactId::Interpreted { kind }.view_name();
        // The acceptance comes first, because the hash the submission
        // names is the one the run's own store answers for — a session
        // handed bytes over, and what the run holds for them is
        // `accept`'s to say. A store that cannot take them is a log that
        // cannot take either fact.
        let (outcome, answer) = match offered {
            Ok(verified) => {
                let accepted = accept(
                    &self.log(),
                    &self.host.run_dir,
                    Some(&self.node),
                    verified.artifact.clone(),
                    &verified.bytes,
                    RecordedOrigin::Submitted,
                )
                .await?;
                (
                    SubmissionOutcome::Accepted {
                        content_hash: accepted.content_hash,
                    },
                    Ok(format!("{name} — accepted. {}{told}", read_as(&verified))),
                )
            }
            Err(crate::artifacts::SubmitError::Refused(report)) => {
                let text = submission_refusal(&report, &name);
                (
                    SubmissionOutcome::Refused { report },
                    Err(RunToolError::Refused { text }),
                )
            }
            Err(other) => return Err(self.not_submitted(other)),
        };
        self.append(EventPayload::Artifacts(ArtifactEvent::Submitted(
            ArtifactSubmittedPayload {
                name: name.clone(),
                artifact_kind: kind,
                outcome,
                probes,
            },
        )))
        .await?;
        answer
    }

    /// Why a submission never became a document to judge, told the way
    /// this session calls the tool that does take it.
    fn not_submitted(&self, error: crate::artifacts::SubmitError) -> RunToolError {
        let reply = Reply::new(error.to_string());
        let text = match error {
            crate::artifacts::SubmitError::Accumulated => reply
                .next(format!(
                    "report each finding with `{}`",
                    self.called(RunTool::PostFinding)
                ))
                .text(),
            _ => reply.text(),
        };
        RunToolError::Refused { text }
    }

    /// Whether this node declares a document of `kind` — which is
    /// whether the tool that submits that kind is offered at all.
    pub(super) fn submits(&self, kind: ArtifactKind) -> bool {
        self.declared.contains(&ArtifactSpec::Interpreted(kind))
    }

    /// A call this tool cannot even read as a submission, told with the
    /// document it does take.
    fn invalid_submission(&self, kind: ArtifactKind, detail: &str) -> RunToolError {
        RunToolError::InvalidSubmission {
            names: format!("`{kind}`"),
            detail: detail.to_string(),
        }
    }

    /// Every artifact this node declares, as a session reads them back in
    /// a refusal.
    fn declared_names(&self) -> String {
        self.declared
            .iter()
            .map(|spec| format!("`{spec}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// One artifact's verdict as the session reads it: what the engine
/// understood, or the numbered problems standing in the way.
fn render_verdict(name: &str, verified: Result<VerifiedArtifact, ArtifactFailure>) -> String {
    match verified {
        Ok(verified) => format!("{name} — ok. {}", read_as(&verified)),
        Err(failure) => match failure.report() {
            Some(report) => numbered(format!("{name} — {}", failure_heading(report)), report),
            None => format!("{name} — {failure}"),
        },
    }
}

/// A document's verdict, and what an acceptance tells the session beside
/// what the engine read out of it.
struct Offered {
    verdict: Result<VerifiedArtifact, crate::artifacts::SubmitError>,
    told: String,
    /// What proving the document ran, task by task.
    probes: Vec<TaskProbe>,
}

impl Offered {
    fn told_nothing(verdict: Result<VerifiedArtifact, crate::artifacts::SubmitError>) -> Self {
        Offered {
            verdict,
            told: String::new(),
            probes: Vec::new(),
        }
    }
}

/// `runs`, gathered under the task each answered for, in the order the
/// tasks first appear.
fn by_task(runs: &[(yunta_core::TaskId, crate::task_cycle::CriterionRun)]) -> Vec<TaskProbe> {
    let mut probes: Vec<TaskProbe> = Vec::new();
    for (task, run) in runs {
        let result = crate::task_cycle::to_results(std::slice::from_ref(run));
        match probes.iter_mut().find(|probe| &probe.task_id == task) {
            Some(probe) => probe.results.extend(result),
            None => probes.push(TaskProbe {
                task_id: task.clone(),
                results: result,
            }),
        }
    }
    probes
}

/// How each test of an accepted spec fails before the work: its exit and
/// the last line it printed, redacted the way the log would be. A test
/// that fails for anything but the missing behavior — a typo, a wrong
/// path — fails after the work too, and this is where its writer sees it.
fn failing_now(
    failing: &[(yunta_core::TaskId, crate::task_cycle::CriterionRun)],
    redactor: &yunta_core::Redactor,
) -> String {
    if failing.is_empty() {
        return String::new();
    }
    let mut text = String::from(
        "\n\nHow each test fails now, before any work — one that fails for anything but the \
         missing behavior fails after the work too:",
    );
    for (task, run) in failing {
        let said = match run.said() {
            Some(said) => format!(" — it said `{}`", redactor.text(&said)),
            None => ", printing nothing".to_string(),
        };
        text.push_str(&format!(
            "\n  `{}` (task `{task}`): exit {}{said}",
            run.cmd,
            run.exit_described()
        ));
    }
    text
}
