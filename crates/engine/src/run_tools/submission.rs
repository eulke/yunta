//! Handing a whole document to the engine, and asking what the close
//! will make of it.
//!
//! A typed artifact is never a file the session writes: it is submitted
//! as an object, validated by the code the node's close runs, and written
//! by the engine once it is accepted. Both calls here go through that one
//! verification for the same reason — a verdict a session can ask for
//! while it can still act has to be the verdict that decides the node, or
//! the first one teaches false confidence.
//!
//! What a session may submit is bounded by what its node declares: the
//! name has to be one of them, under the kind the tool submits, so a
//! document the close would never look for has no way in.

use serde_json::Value;
use yunta_core::events::{
    ArtifactOrigin, ArtifactSubmittedPayload, EventPayload, SubmissionOutcome,
};
use yunta_core::{ArtifactKind, ArtifactSpec};

use crate::artifacts::{accept, Declared};

use super::session::{RunToolError, SessionTools};
use super::verdicts::{backticked, failure_heading, numbered, read_as, submission_refusal};

impl SessionTools {
    /// The verdict this node's close will reach, while the session can
    /// still act on it.
    ///
    /// Runs `verify_one` — the close's own verification, not a second
    /// reading of it. What comes back on success is what the engine
    /// understood, not just that the file parsed: a tasks document that reads as
    /// six tasks when the session meant seven is a failure nothing else
    /// catches.
    pub(super) fn check_artifact(
        &self,
        args: &serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let wanted = args.get("name").and_then(Value::as_str);
        let specs: Vec<&ArtifactSpec> = self
            .declared
            .iter()
            .filter(|spec| wanted.is_none_or(|name| spec.name() == name))
            .collect();
        if specs.is_empty() {
            return Err(self.nothing_to_check(wanted));
        }
        let verdicts: Vec<String> = specs.into_iter().map(|spec| self.verdict(spec)).collect();
        Ok(verdicts.join("\n\n"))
    }

    /// What this node's close would say about one declared artifact right
    /// now.
    fn verdict(&self, spec: &ArtifactSpec) -> String {
        match crate::artifacts::verify_one(
            &self.node,
            spec,
            &self.host.run_dir,
            self.host.max_artifact_bytes,
        ) {
            Ok(verified) => format!("{} — ok. {}", spec.name(), read_as(&verified)),
            Err(failure) => match failure.report() {
                Some(report) => numbered(
                    format!("{} — {}", spec.name(), failure_heading(report)),
                    report,
                ),
                None => format!("{} — {failure}", spec.name()),
            },
        }
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
    /// kind the node declared and, once accepted, writes the file the
    /// close reads.
    pub(super) async fn submit(
        &self,
        kind: ArtifactKind,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let spec = self.submitted_spec(kind, &args)?;
        let Some(document) = args.get("document").filter(|value| value.is_object()) else {
            return Err(self.invalid_submission(kind, "`document` is missing or is not an object"));
        };
        let name = spec.name().to_string();
        let written = crate::artifacts::submit(
            spec,
            &self.host.run_dir,
            document.clone(),
            self.host.max_artifact_bytes,
        );
        self.record(name, kind, written).await
    }

    /// Which artifact a submission is about: a name this node declares,
    /// under the kind this tool submits.
    fn submitted_spec(
        &self,
        kind: ArtifactKind,
        args: &serde_json::Map<String, Value>,
    ) -> Result<&ArtifactSpec, RunToolError> {
        let Some(name) = args.get("name").and_then(Value::as_str) else {
            return Err(self.invalid_submission(kind, "`name` is missing"));
        };
        let Some(spec) = self.declared.iter().find(|spec| spec.name() == name) else {
            return Err(RunToolError::UndeclaredArtifact {
                name: name.to_string(),
                declared: self.declared_names(),
            });
        };
        match self.wrong_kind(spec, kind) {
            Some(wrong) => Err(wrong),
            None => Ok(spec),
        }
    }

    /// Records the engine's verdict on a submitted document and answers
    /// the session with it: what was read out of the file on acceptance,
    /// what to fix on a refusal. Both are events — a run reads back every
    /// document a session offered, not only the ones that landed.
    ///
    /// Two facts, not one: `artifact_submitted` is the call this session
    /// made and how it was answered, and it is on the log whether the
    /// document landed or not. An accepted document is also an artifact
    /// the run now holds, which is what [`accept`] states — so a reader
    /// asking what the run holds never has to know that a session is
    /// what handed it over.
    async fn record(
        &self,
        name: String,
        kind: ArtifactKind,
        written: Result<crate::artifacts::VerifiedArtifact, crate::artifacts::SubmitError>,
    ) -> Result<String, RunToolError> {
        let (outcome, answer, accepted) = match written {
            Ok(verified) => (
                SubmissionOutcome::Accepted {
                    content_hash: verified.content_hash.clone(),
                },
                Ok(format!("{name} — accepted. {}", read_as(&verified))),
                Some(verified),
            ),
            Err(crate::artifacts::SubmitError::Refused(report)) => {
                let text = submission_refusal(&report, &name);
                (
                    SubmissionOutcome::Refused { report },
                    Err(RunToolError::Refused { text }),
                    None,
                )
            }
            Err(other) => {
                return Err(RunToolError::Refused {
                    text: other.to_string(),
                })
            }
        };
        self.append(EventPayload::ArtifactSubmitted(ArtifactSubmittedPayload {
            name: name.clone(),
            artifact_kind: kind,
            outcome,
        }))
        .await?;
        if let Some(verified) = accepted {
            accept(
                &self.log(),
                &self.host.run_dir,
                Some(&self.node),
                Declared {
                    name: &name,
                    kind: Some(kind),
                },
                &verified.bytes,
                ArtifactOrigin::Submitted,
            )
            .await?;
        }
        answer
    }

    /// The names this node declares under `kind`, in declaration order.
    pub(super) fn submittable(&self, kind: ArtifactKind) -> impl Iterator<Item = &str> {
        self.declared.iter().filter_map(move |spec| match spec {
            ArtifactSpec::Typed {
                name,
                kind: declared,
            } if *declared == kind => Some(name.as_str()),
            _ => None,
        })
    }

    /// The refusal for a name this node declares under something other
    /// than the kind the tool submits, naming the tool that does take it.
    fn wrong_kind(&self, spec: &ArtifactSpec, kind: ArtifactKind) -> Option<RunToolError> {
        let (declared, expected) = match spec {
            ArtifactSpec::Typed { kind: declared, .. } if *declared == kind => return None,
            ArtifactSpec::Typed { kind: declared, .. } => (
                declared.to_string(),
                declared
                    .submit_tool()
                    .map(str::to_string)
                    .unwrap_or_else(|| ArtifactKind::POST_FINDING_TOOL.to_string()),
            ),
            ArtifactSpec::Plain(_) => (
                "no kind (a file this session writes)".to_string(),
                String::new(),
            ),
        };
        Some(RunToolError::WrongKind {
            name: spec.name().to_string(),
            declared,
            expected,
        })
    }

    /// A call this tool cannot even read as a submission, told with the
    /// names it does take.
    fn invalid_submission(&self, kind: ArtifactKind, detail: &str) -> RunToolError {
        let names: Vec<&str> = self.submittable(kind).collect();
        RunToolError::InvalidSubmission {
            names: backticked(&names),
            detail: detail.to_string(),
        }
    }

    /// Every artifact this node declares, as a session reads them back in
    /// a refusal.
    fn declared_names(&self) -> String {
        self.declared
            .iter()
            .map(|spec| format!("`{}`", spec.name()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}
