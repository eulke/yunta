//! What each tool the control plane advertises actually does.
//!
//! One function per tool, each the in-process equivalent of the command
//! a person would run — same `Context`, same engine call, same refusal
//! — so an agent client and a terminal reach the same verdict.

use std::path::Path;

use serde_json::Value;
use yunta_core::{AdapterId, ModeName, RunId};

use crate::context::Context;
use crate::error::CliError;
use crate::shell::Shell;

fn required_str<'a>(
    args: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<&'a str, CliError> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::msg(format!("missing or non-string argument `{name}`")))
}

/// An optional string argument: absent or `null` is `None`, and a value of
/// another type is refused by name rather than read as if it were not sent.
fn optional_str<'a>(
    args: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<Option<&'a str>, CliError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(CliError::msg(format!("non-string argument `{name}`"))),
    }
}

/// The `run_id` argument as the id it has to be, so a call naming
/// something that is not one is answered before any disk is read.
fn required_run_id(args: &serde_json::Map<String, Value>) -> Result<RunId, CliError> {
    Ok(required_str(args, "run_id")?.parse()?)
}

pub(super) async fn tool_list_workflows(shell: &Shell) -> Result<String, CliError> {
    // The same catalog `yunta list` renders, read in-process: shelling
    // out to a subprocess would print onto this server's own stdout — the
    // very stream its JSON-RPC replies travel on.
    let catalog = shell.workspace().catalog().await;
    // An agent reads the catalog as Markdown: no terminal's width or
    // locale decides what it is told.
    use surco::surface::{Markdown, Surface};
    use yunta_render::page::Page;
    Ok(Markdown.draw(&catalog.document(&Markdown.look())))
}

pub(super) async fn tool_workflow_status(
    shell: &Shell,
    args: &serde_json::Map<String, Value>,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let ctx = Context::of(shell)?;
    let reading = ctx.project.run(&run_id).await?.reading().await?;
    // The same versioned DTO `yunta status --json` prints, serialized to
    // the tool result rather than to stdout, and read at this server's
    // own injected clock.
    yunta_render::document::to_json_string(&yunta_render::document::RunDocument::of(
        &reading.run.id,
        &reading.run.events,
        &reading.state,
        &reading.run.manifest.doc,
        &reading.frame,
        reading.decision.clone(),
    ))
    .map_err(CliError::msg)
}

pub(super) async fn tool_run_workflow(
    shell: &Shell,
    args: &serde_json::Map<String, Value>,
) -> Result<String, CliError> {
    let name = required_str(args, "workflow")?;
    let mut inputs: Vec<String> = Vec::new();
    match args.get("inputs") {
        None | Some(Value::Null) => {}
        Some(Value::Object(object)) => {
            for (key, value) in object {
                let rendered = match value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                inputs.push(format!("{key}={rendered}"));
            }
        }
        Some(_) => return Err(CliError::msg("non-object argument `inputs`")),
    }
    let adapter = optional_str(args, "adapter")?
        .map(str::parse::<AdapterId>)
        .transpose()?;
    let mode = optional_str(args, "mode")?
        .map(str::parse::<ModeName>)
        .transpose()?;

    // Resolves `name` against the whole catalog — the repo's own
    // `.yunta/workflows/` then a publisher's vendored packs
    // (`acme/review`) — then creates the run and hands it off, all
    // in-process through the same `start_detached` `yunta run --detach`
    // calls.
    let ctx = Context::of(shell)?;
    let started = super::super::run::start_detached(
        &ctx,
        Path::new(name),
        &inputs,
        adapter.as_ref(),
        mode.as_ref(),
    )
    .await?;
    // The run id first, so a client that reads one line still reads the
    // thing it asked for, and each pre-run warning under it: a client
    // that starts runs is the one deciding whether the run is worth
    // starting, and stderr never reaches it.
    Ok(std::iter::once(format!("run_id: {}", started.run_id))
        .chain(started.warnings.lines())
        .collect::<Vec<_>>()
        .join("\n"))
}

pub(super) async fn tool_resume_run(
    shell: &Shell,
    args: &serde_json::Map<String, Value>,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let ctx = Context::of(shell)?;
    let open = ctx.project.run(&run_id).await?;
    // Said here because the process below would say it only to `detached.log`,
    // which no caller of this tool reads.
    open.refuse_if_driven().await?;
    // A run paused on a limit's reset is handed to a resume that waits for
    // the reset first, so it does not spend the limit before it lifts.
    open.resume_detached().await?;
    Ok(format!(
        "run {run_id}: resumed, driving forward independently"
    ))
}

pub(super) async fn tool_resolve_gate(
    shell: &Shell,
    args: &serde_json::Map<String, Value>,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let option = required_str(args, "option")?.parse::<yunta_core::OptionId>()?;
    let by = optional_str(args, "by")?
        .map(str::parse::<yunta_core::Responder>)
        .transpose()?;
    let text = optional_str(args, "text")?.map(str::to_string);

    // The command's own path, called rather than copied: a client that
    // answers a gate records exactly what a person answering it records,
    // under this server's own injected clock, and reads back the same
    // sentence.
    let ctx = Context::of(shell)?;
    super::super::resolve_gate::resolve(&ctx, &run_id, &option, by.as_ref(), text.as_deref()).await
}

pub(super) async fn tool_answer_questions(
    shell: &Shell,
    args: &serde_json::Map<String, Value>,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let node = required_str(args, "node")?.parse::<yunta_core::NodeId>()?;
    let by = optional_str(args, "by")?
        .map(str::parse::<yunta_core::Responder>)
        .transpose()?;
    let answers = answers_from(args)?;

    let ctx = Context::of(shell)?;
    let open = ctx.project.run(&run_id).await?;
    let reply = yunta_core::port::QuestionsReply {
        answers,
        channel: yunta_core::events::Channel::Mcp,
        responder: by,
    };
    open.answer(&node, reply).await?;
    Ok(format!(
        "run {run_id}: node `{node}` answered, driving forward independently"
    ))
}

/// The `answers` argument as the engine takes it: one entry per
/// question, each naming the question it answers.
///
/// Only the shape of the argument is checked here — that it is a list
/// of `{id, value}` with an id that is one. Whether those answers
/// satisfy the questions is the document's own judgement, and it is
/// made once, inside the engine, so this surface and the console reach
/// the same verdict.
fn answers_from(
    args: &serde_json::Map<String, Value>,
) -> Result<Vec<yunta_core::Answer>, CliError> {
    let Some(entries) = args.get("answers").and_then(Value::as_array) else {
        return Err(CliError::msg(
            "`answers` is required: a list of `{id, value}`, one per question",
        ));
    };
    entries
        .iter()
        .map(|entry| {
            let id = entry
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::msg("every `answers` entry names the `id` it answers"))?
                .parse::<yunta_core::QuestionId>()?;
            // A value arrives as whatever JSON the client had for it
            // and is recorded as the text the document is written in —
            // the same text a person types at the console, so a `3`
            // and a "3" are one answer.
            let value = match entry.get("value") {
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => return Err(CliError::msg(format!("answer `{id}` names no `value`"))),
            };
            Ok(yunta_core::Answer { id, value })
        })
        .collect()
}
