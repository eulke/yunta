//! The `AskQuestions` schedule step: a node the
//! log derives as waiting-on-questions gets its questions put to the
//! human through `HumanInteraction::ask` — the ONE ask site, serving the
//! first invocation (right after the node closes waiting) and every
//! later `yunta resume` through the identical path, with zero
//! conversational state: the questions are re-read from the artifact on
//! disk, never from memory.

use yunta_core::events::{ArtifactOrigin, EventPayload};
use yunta_core::{ArtifactKind, ArtifactSpec, Node};

use crate::artifacts::{accept, Declared, ANSWERS_SUFFIX};

use super::node_close::write_progress;
use super::{RunCtx, RunError};

/// What the ask round produced: the node was fully answered (and closed
/// with `node_started` + `questions_answered` + `node_finished`), or a
/// reason to pause — no surface, or an invalid reply, each citing
/// exactly what's missing.
pub(super) enum AskOutcome {
    Answered,
    Pause { reason: String },
}

pub(super) async fn execute_ask(ctx: &RunCtx<'_>, node: &Node) -> Result<AskOutcome, RunError> {
    // The declared questions artifacts, re-read from the frozen run.dir
    // — artifacts are immutable once written — the node produced them
    // before it closed waiting.
    let mut question_files: Vec<(String, yunta_core::QuestionsFile)> = Vec::new();
    if let Some(artifacts) = &node.artifacts {
        for spec in &artifacts.produces {
            let ArtifactSpec::Typed { name, kind } = spec else {
                continue;
            };
            if !matches!(kind, ArtifactKind::Questions) {
                continue;
            }
            let relative = std::path::Path::new(yunta_core::ARTIFACTS_DIR).join(name);
            let bytes =
                std::fs::read(ctx.run_dir.join(&relative)).map_err(|source| RunError::Io {
                    context: format!("read questions artifact `{}`", relative.display()),
                    source,
                })?;
            // The same door `close_artifacts` reads a questions file
            // through: the ask round names what is wrong with the
            // document, never what a deserializer made of it.
            let file = yunta_core::shape::read::<yunta_core::QuestionsFile>(
                &bytes,
                relative.display().to_string(),
            )?;
            question_files.push((name.clone(), file));
        }
    }

    // Collect every reply first — nothing lands on the log until the
    // whole round is valid, so a half-answered node never records a
    // partial resolution.
    let mut replies = Vec::new();
    let mut unanswered: Vec<String> = Vec::new();
    for (name, file) in &question_files {
        match ctx.human_interaction.ask(file, node.interactive).await {
            None => {
                // No surface (headless, `yunta test`) — cite every id.
                unanswered.extend(file.questions.iter().map(|q| q.id.to_string()));
            }
            Some(reply) => {
                let violations = yunta_core::validate_answers(file, &reply.answers);
                if violations.is_empty() {
                    replies.push((name.clone(), reply));
                } else {
                    // The surface answered but the reply doesn't satisfy
                    // the questions' own declared rules — the engine is
                    // the verdict-giver, so an invalid reply is refused
                    // citing the exact violations, never half-recorded.
                    unanswered.extend(violations);
                }
            }
        }
    }
    if !unanswered.is_empty() {
        return Ok(AskOutcome::Pause {
            reason: format!(
                "node `{}` asked {} question(s) awaiting an answer: {}",
                node.id,
                unanswered.len(),
                unanswered.join(", ")
            ),
        });
    }

    // Everything answered and valid: the node's remaining work — the
    // answers — happens now, as its own (session-less) attempt on the
    // log: re-open, materialize each answers artifact (engine-written)
    // with its `questions_answered` (hash + channel + responder),
    // close finished.
    let attempt = ctx
        .load_events()
        .await?
        .iter()
        .filter(|e| {
            e.node_id.as_ref() == Some(&node.id)
                && matches!(e.payload(), Some(EventPayload::NodeStarted(_)))
        })
        .count() as u32
        + 1;
    ctx.emit(
        Some(&node.id),
        EventPayload::NodeStarted(yunta_core::events::NodeStartedPayload { attempt }),
    )
    .await?;
    for (name, reply) in replies {
        let answers_name = format!("{name}{ANSWERS_SUFFIX}");
        let answers_abs = ctx
            .run_dir
            .join(yunta_core::ARTIFACTS_DIR)
            .join(&answers_name);
        let answers_file = yunta_core::AnswersFile {
            answers: reply.answers,
        };
        let bytes = yunta_core::yaml::to_string(&answers_file)
            .map_err(|e| RunError::ManifestWrite {
                path: answers_abs.clone(),
                detail: e.to_string(),
            })?
            .into_bytes();
        // The answers sit beside the questions they answer, which is
        // where the next node's `artifact:` context source reads them.
        std::fs::write(&answers_abs, &bytes).map_err(|source| RunError::Io {
            context: format!("write `{}`", answers_abs.display()),
            source,
        })?;
        let accepted = accept(
            &ctx.log(),
            ctx.run_dir,
            Some(&node.id),
            Declared {
                name: &answers_name,
                kind: None,
            },
            &bytes,
            ArtifactOrigin::Answered,
        )
        .await?;
        ctx.emit(
            Some(&node.id),
            EventPayload::QuestionsAnswered(yunta_core::events::QuestionsAnsweredPayload {
                answers_hash: accepted.content_hash,
                channel: reply.channel,
                responder: reply.responder,
            }),
        )
        .await?;
    }
    ctx.emit(
        Some(&node.id),
        EventPayload::NodeFinished(yunta_core::events::NodeFinishedPayload {
            outcome: "questions answered".to_string(),
            tokens_used: Default::default(),
        }),
    )
    .await?;
    write_progress(ctx).await?;
    Ok(AskOutcome::Answered)
}
