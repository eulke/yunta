//! Reporting, changing and taking back one finding.
//!
//! A finding reaches the run through these three calls and never through
//! a file the session writes: the engine holds each one to the findings
//! document's own rules the moment it arrives, so a session learns what
//! is wrong while it can still fix it, and what it already reported is
//! never at risk. Where an id stands for this node — free, live or
//! withdrawn — is derived from the log on every call, which is why a
//! resumed session cannot reuse an id it withdrew before the crash.
//!
//! Every refusal is an event too. A session that argues with the rules
//! leaves a trace the run can be read back from.

use serde_json::Value;
use yunta_core::diagnostic::{Diagnostic, DocumentRef, Named, Problem, Report, RuleCode, Subject};
use yunta_core::events::findings::{FindingLedger, Slot};
use yunta_core::events::{
    EventPayload, Finding, FindingOperation, FindingPostedPayload, FindingRefusedPayload,
    FindingUpdatedPayload, FindingWithdrawnPayload,
};
use yunta_core::{ArtifactKind, FindingEntry, FindingId, FindingsFile, Withdrawal};

use super::session::{RunToolError, SessionTools};
use super::verdicts::refusal;

impl SessionTools {
    pub(super) async fn post_finding(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let entry = self.vetted(FindingOperation::Post, args).await?;
        let id = entry.id.clone();
        self.append(EventPayload::FindingPosted(FindingPostedPayload {
            finding: Finding::from(entry),
        }))
        .await?;
        Ok(format!("finding `{id}` recorded"))
    }

    pub(super) async fn update_finding(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let entry = self.vetted(FindingOperation::Update, args).await?;
        let id = entry.id.clone();
        self.append(EventPayload::FindingUpdated(FindingUpdatedPayload {
            finding: Finding::from(entry),
        }))
        .await?;
        Ok(format!("finding `{id}` updated"))
    }

    pub(super) async fn withdraw_finding(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let operation = FindingOperation::Withdraw;
        let document = DocumentRef::new(ArtifactKind::Findings, tool_of(operation));
        let withdrawal: Withdrawal = match serde_path_to_error::deserialize(Value::Object(args)) {
            Ok(withdrawal) => withdrawal,
            Err(error) => {
                let report = Report::new(document, vec![parse_problem(&error)]);
                return Err(self.refuse(operation, None, report).await);
            }
        };
        let id = withdrawal.id.clone();
        let mut broken = withdrawal.check();
        let standing = self.finding_status(&id).await?;
        broken.extend(Self::id_rule(operation, &id, standing.as_ref()));
        if !broken.is_empty() {
            let report = Report::new(document, broken);
            return Err(self.refuse(operation, Some(id), report).await);
        }
        self.append(EventPayload::FindingWithdrawn(FindingWithdrawnPayload {
            id: id.clone(),
            reason: withdrawal.reason,
        }))
        .await?;
        Ok(format!("finding `{id}` withdrawn"))
    }

    /// The finding a `post` or an `update` offers, once it has passed the
    /// findings document's rules and the id rule for that operation.
    ///
    /// The two calls carry the same document and differ only in what the
    /// id is allowed to be, so they are held to the rules in one place.
    async fn vetted(
        &self,
        operation: FindingOperation,
        args: serde_json::Map<String, Value>,
    ) -> Result<FindingEntry, RunToolError> {
        let entry = match self.entry(args) {
            Ok(entry) => entry,
            Err(report) => return Err(self.refuse(operation, None, report).await),
        };
        let id = entry.id.clone();
        let standing = self.finding_status(&id).await?;
        if let Some(broken) = Self::id_rule(operation, &id, standing.as_ref()) {
            let report = Report::new(
                DocumentRef::new(ArtifactKind::Findings, tool_of(operation)),
                vec![broken],
            );
            return Err(self.refuse(operation, Some(id), report).await);
        }
        Ok(entry)
    }

    /// Reads one finding a session offered, against the same type and
    /// the same rules the findings artifact is held to.
    ///
    /// The strict entry type rather than the log's own: what a log reads
    /// back tolerates a key a later writer added, and a session handing
    /// over a finding is the frontier where a key nobody declared is a
    /// mistake to name rather than a field to drop.
    fn entry(&self, args: serde_json::Map<String, Value>) -> Result<FindingEntry, Report> {
        let document = DocumentRef::new(ArtifactKind::Findings, ArtifactKind::POST_FINDING_TOOL);
        let entry: FindingEntry = serde_path_to_error::deserialize(Value::Object(args))
            .map_err(|error| Report::new(document.clone(), vec![parse_problem(&error)]))?;
        // One entry is a whole findings document as far as the rules are
        // concerned: every rule of that document is about an entry.
        let broken = yunta_core::shape::Document::check(&FindingsFile {
            findings: vec![entry.clone()],
        });
        if broken.is_empty() {
            Ok(entry)
        } else {
            Err(Report::new(document, broken))
        }
    }

    /// Where `id` stands for this node, folded from the run's own log.
    async fn finding_status(&self, id: &FindingId) -> Result<Option<Slot>, RunToolError> {
        let events = self.events().await?;
        Ok(FindingLedger::of(&events).status(&self.node, id).cloned())
    }

    /// The rule a finding call breaks by naming the id it named.
    ///
    /// A refusal here is a rule of the findings document like any other,
    /// so a session hears about an id the same way it hears about an
    /// empty `detail` — one shape of refusal, one kind of event.
    fn id_rule(
        operation: FindingOperation,
        id: &FindingId,
        standing: Option<&Slot>,
    ) -> Option<Diagnostic> {
        let broken = |code: RuleCode, detail: &str| {
            Some(Diagnostic::new(
                Subject::Finding(Named::new(id.clone(), 0)),
                Problem::rule(code, detail),
            ))
        };
        match (operation, standing) {
            (FindingOperation::Post, None) => None,
            (FindingOperation::Post, Some(Slot::Live(_))) => broken(
                RuleCode::DuplicateId,
                "this node already reported a finding under that id; report the new one \
                 under an id of its own, or update the one that stands",
            ),
            (_, Some(Slot::Withdrawn { .. })) => broken(
                RuleCode::WithdrawnId,
                "this node withdrew that id, and a withdrawal is final; a finding that \
                 comes back is a new id",
            ),
            (_, Some(Slot::Live(_))) => None,
            (FindingOperation::Update | FindingOperation::Withdraw, None) => broken(
                RuleCode::UnknownId,
                "this node never reported a finding under that id; only the node that \
                 reported one can change it",
            ),
        }
    }

    /// Records a refused finding call and answers the session with the
    /// problems to fix.
    ///
    /// A log the refusal cannot reach is the answer instead: a session
    /// told to fix a finding whose refusal went unrecorded would be acting
    /// on a run state nothing else can see.
    async fn refuse(
        &self,
        operation: FindingOperation,
        id: Option<FindingId>,
        report: Report,
    ) -> RunToolError {
        let text = refusal(operation, &report);
        match self
            .append(EventPayload::FindingRefused(FindingRefusedPayload {
                operation,
                id,
                report,
            }))
            .await
        {
            Ok(()) => RunToolError::Refused { text },
            Err(unreachable_log) => unreachable_log,
        }
    }
}

/// The tool a finding operation is called through — the name a refusal
/// about that call points at.
fn tool_of(operation: FindingOperation) -> &'static str {
    match operation {
        FindingOperation::Post => ArtifactKind::POST_FINDING_TOOL,
        FindingOperation::Update => ArtifactKind::UPDATE_FINDING_TOOL,
        FindingOperation::Withdraw => ArtifactKind::WITHDRAW_FINDING_TOOL,
    }
}

/// What a session offered that the document's own type could not read,
/// pointed at the field that could not be read — never the
/// deserializer's account of itself.
fn parse_problem(error: &serde_path_to_error::Error<serde_json::Error>) -> Diagnostic {
    Diagnostic::new(
        Subject::Document,
        Problem::parse(error.path().to_string(), error.inner().to_string()),
    )
}
