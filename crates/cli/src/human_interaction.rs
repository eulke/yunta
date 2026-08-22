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

pub struct ConsoleInteraction;

/// Reads one line from stdin; `None` on EOF (stdin closed mid-prompt —
/// the same "can't interact" case as never having a TTY, never a
/// silent default).
fn read_line() -> Option<String> {
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
        return None;
    }
    Some(line.trim().to_string())
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
            print!("choose an option id: ");
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            // EOF (0 bytes read) means stdin closed mid-prompt — same
            // "can't interact" case as never having a TTY at all, not a
            // silent default.
            if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
                return None;
            }
            let line = line.trim();
            if escalation.options.iter().any(|o| o.id == line) {
                break line.to_string();
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

        print!("optional free-text feedback (enter to skip): ");
        let _ = std::io::stdout().flush();
        let mut free_text_line = String::new();
        if std::io::stdin().read_line(&mut free_text_line).unwrap_or(0) == 0 {
            return None;
        }
        let free_text = free_text_line.trim();

        Some(GateResolvedPayload {
            chosen_option: Some(chosen_option),
            resolved_by: std::env::var("USER").ok(),
            free_text: (!free_text.is_empty()).then(|| free_text.to_string()),
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
                match question.answer_type {
                    AnswerType::Text => print!("{} ", question.text),
                    AnswerType::Choice => {
                        print!("{} [{}] ", question.text, question.values.join("/"))
                    }
                    AnswerType::Boolean => print!("{} [y/n] ", question.text),
                }
                if !question.required {
                    print!("(enter to skip) ");
                }
                let _ = std::io::stdout().flush();
                let line = read_line()?;
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
