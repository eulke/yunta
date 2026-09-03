//! `ConsoleInteraction` — the TTY implementation of
//! `yunta_engine::HumanInteraction`. Renders the escalation object
//! exactly as the engine built it (summary, mechanical evidence,
//! options with their mandatory tradeoff) and reads a decision from
//! stdin. Never auto-decides: on a non-interactive stdin (piped, no
//! TTY, redirected from `/dev/null`) it reports "can't interact"
//! (`None`) rather than guessing, and the caller degrades to pausing —
//! the same rule `kind: questions` already applies.

use std::io::{IsTerminal, Write};

use async_trait::async_trait;
use yunta_core::events::{Channel, GateResolvedPayload, GateWaitingPayload};
use yunta_core::{Answer, AnswerType, QuestionsFile};
use yunta_engine::{HumanInteraction, QuestionsReply};

use crate::error::warn;

pub struct ConsoleInteraction;

/// Prints `message`, flushes it, and reads one trimmed line from stdin —
/// the single prompt the console surface uses. The read runs on a blocking
/// thread (`spawn_blocking`), so a person taking their time answering never
/// freezes the run's single-threaded runtime and the tasks that share it:
/// the per-node run-tools listener, a `--follow` follower, the Ctrl-C
/// handler.
///
/// `None` means no answer could be read, and the caller degrades to
/// pausing. The two ways that happens are kept distinct: a clean EOF
/// (stdin closed mid-prompt) is silent, the same "can't interact" case as
/// having no TTY; a real read failure is surfaced with a `warning:` so it
/// is never mistaken for one.
async fn prompt(message: &str) -> Option<String> {
    print!("{message}");
    let _ = std::io::stdout().flush();
    let read = tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map(|read| (read, line))
    })
    .await;
    match read {
        Ok(Ok((0, _))) => None,
        Ok(Ok((_, line))) => Some(line.trim().to_string()),
        Ok(Err(e)) => {
            warn(format!("could not read your answer from stdin: {e}"));
            None
        }
        Err(e) => {
            warn(format!("the stdin reader task failed: {e}"));
            None
        }
    }
}

#[async_trait]
impl HumanInteraction for ConsoleInteraction {
    async fn resolve(&self, escalation: &GateWaitingPayload) -> Option<GateResolvedPayload> {
        if !std::io::stdin().is_terminal() {
            return None;
        }

        println!();
        println!("=== gate: a decision is needed ===");
        println!("{}", escalation.summary);
        if !escalation.evidence.is_empty() {
            println!();
            println!("evidence: {}", escalation.evidence);
        }
        println!();
        println!("options:");
        for option in &escalation.options {
            println!("  {} — {}", option.id, option.label);
            println!("      tradeoff: {}", option.tradeoff);
        }

        let chosen_option = loop {
            let line = prompt("choose an option id: ").await?;
            if escalation.options.iter().any(|o| o.id == line) {
                break line;
            }
            println!(
                "`{line}` isn't one of: {}",
                escalation
                    .options
                    .iter()
                    .map(|o| o.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        };

        let free_text = prompt("optional free-text feedback (enter to skip): ").await?;

        Some(GateResolvedPayload {
            chosen_option: Some(chosen_option),
            resolved_by: std::env::var("USER").ok(),
            free_text: (!free_text.is_empty()).then_some(free_text),
            approved_sha: None,
        })
    }

    /// Question by question over the TTY, honoring each `answer_type`
    /// at input time (the engine re-validates the
    /// whole reply anyway — the surface's checks are UX, the engine's
    /// are the verdict). A non-required question accepts an empty line
    /// as "no answer"; a required one re-asks.
    async fn ask(&self, questions: &QuestionsFile, _interactive: bool) -> Option<QuestionsReply> {
        if !std::io::stdin().is_terminal() {
            return None;
        }

        println!();
        println!(
            "=== questions: {} answer(s) needed ===",
            questions.questions.len()
        );
        let mut answers = Vec::new();
        for question in &questions.questions {
            let value = loop {
                let mut message = match question.answer_type {
                    AnswerType::Text => format!("{} ", question.text),
                    AnswerType::Choice => {
                        format!("{} [{}] ", question.text, question.values.join("/"))
                    }
                    AnswerType::Boolean => format!("{} [y/n] ", question.text),
                };
                if !question.required {
                    message.push_str("(enter to skip) ");
                }
                let line = prompt(&message).await?;
                if line.is_empty() {
                    if question.required {
                        println!("`{}` is required", question.id);
                        continue;
                    }
                    break None;
                }
                match question.answer_type {
                    AnswerType::Text => break Some(line),
                    AnswerType::Choice => {
                        if question.values.contains(&line) {
                            break Some(line);
                        }
                        println!("`{line}` isn't one of: {}", question.values.join(", "));
                    }
                    AnswerType::Boolean => match line.as_str() {
                        "y" | "yes" | "true" => break Some("true".to_string()),
                        "n" | "no" | "false" => break Some("false".to_string()),
                        _ => println!("answer y or n"),
                    },
                }
            };
            if let Some(value) = value {
                answers.push(Answer {
                    id: question.id.clone(),
                    value,
                });
            }
        }
        Some(QuestionsReply {
            answers,
            channel: Channel::Tty,
            responder: std::env::var("USER").ok(),
        })
    }
}
