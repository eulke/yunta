//! What each tool the control plane advertises actually does.
//!
//! One function per tool, each the in-process equivalent of the command
//! a person would run — same `Context`, same engine call, same refusal
//! — so an agent client and a terminal reach the same verdict.

use std::path::Path;

use serde_json::Value;
use yunta_core::{AdapterId, Clock, ModeName, RunId};

use crate::context::Context;
use crate::error::CliError;
use crate::interrupt::Interrupt;

fn required_str<'a>(
    args: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<&'a str, CliError> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::msg(format!("missing or non-string argument `{name}`")))
}

/// The `run_id` argument as the id it has to be, so a call naming
/// something that is not one is answered before any disk is read.
fn required_run_id(args: &serde_json::Map<String, Value>) -> Result<RunId, CliError> {
    Ok(required_str(args, "run_id")?.parse()?)
}

pub(super) async fn tool_list_workflows(
    cwd: &Path,
    interrupt: Interrupt,
) -> Result<String, CliError> {
    // The same catalog `yunta list` renders, built in-process: shelling
    // out to a subprocess would print onto this server's own stdout — the
    // very stream its JSON-RPC replies travel on. Best-effort storage, so
    // a repo with no state root yet still lists, just without estimates.
    let ctx = Context::resolve_in(cwd.to_path_buf(), interrupt).ok();
    Ok(super::super::list::render_catalog(cwd, ctx.as_ref()).await)
}

pub(super) async fn tool_workflow_status(
    cwd: &Path,
    args: &serde_json::Map<String, Value>,
    interrupt: Interrupt,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let ctx = Context::resolve_in(cwd.to_path_buf(), interrupt)?;
    let open = ctx.open_run(&run_id).await?;
    let (events, manifest) = (open.events, open.manifest.doc);
    // The same versioned DTO `yunta status --json` prints, serialized to
    // the tool result rather than to stdout, and read at this server's
    // own injected clock.
    crate::json::to_json_string(&crate::json::RunDocument::of(
        &run_id,
        &events,
        &manifest,
        ctx.clock.now(),
    ))
    .map_err(CliError::msg)
}

pub(super) async fn tool_run_workflow(
    cwd: &Path,
    args: &serde_json::Map<String, Value>,
    interrupt: Interrupt,
) -> Result<String, CliError> {
    let name = required_str(args, "workflow")?;
    let mut inputs: Vec<String> = Vec::new();
    if let Some(object) = args.get("inputs").and_then(Value::as_object) {
        for (key, value) in object {
            let rendered = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            inputs.push(format!("{key}={rendered}"));
        }
    }
    let adapter = args
        .get("adapter")
        .and_then(Value::as_str)
        .map(str::parse::<AdapterId>)
        .transpose()?;
    let mode = args
        .get("mode")
        .and_then(Value::as_str)
        .map(str::parse::<ModeName>)
        .transpose()?;

    // Resolves `name` against the whole catalog — the repo's own
    // `.yunta/workflows/` then a publisher's vendored packs
    // (`acme/review`) — then creates the run and hands it off, all
    // in-process through the same `start_detached` `yunta run --detach`
    // calls.
    let ctx = Context::resolve_in(cwd.to_path_buf(), interrupt)?;
    let storage = ctx.async_storage().await?;
    let started = super::super::run::start_detached(
        &ctx,
        &storage,
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
        .chain(started.warnings.lines().map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n"))
}

pub(super) async fn tool_resume_run(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
    interrupt: Interrupt,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let ctx = Context::resolve_in(cwd.to_path_buf(), interrupt)?;
    let run_dir = ctx.open_run(&run_id).await?.run_dir;
    super::super::spawn_detached_resume(&run_dir, run_id.as_str(), cwd)
        .await
        .map_err(|source| super::super::DetachedResumeError::new(&run_id, source))?;
    Ok(format!(
        "run {run_id}: resumed, driving forward independently"
    ))
}

pub(super) async fn tool_resolve_gate(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
    interrupt: Interrupt,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let option = required_str(args, "option")?.parse::<yunta_core::OptionId>()?;
    let by = args
        .get("by")
        .and_then(Value::as_str)
        .map(str::parse::<yunta_core::Responder>)
        .transpose()?;
    let text = args.get("text").and_then(Value::as_str).map(str::to_string);

    // The command's own path, called rather than copied: a client that
    // answers a gate records exactly what a person answering it records,
    // under this server's own injected clock, and reads back the same
    // sentence.
    let ctx = Context::resolve_in(cwd.to_path_buf(), interrupt)?;
    super::super::resolve_gate::resolve(&ctx, &run_id, &option, by.as_ref(), text.as_deref()).await
}

pub(super) async fn tool_answer_questions(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
    interrupt: Interrupt,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let node = required_str(args, "node")?.parse::<yunta_core::NodeId>()?;
    let by = args
        .get("by")
        .and_then(Value::as_str)
        .map(str::parse::<yunta_core::Responder>)
        .transpose()?;
    let answers = answers_from(args)?;

    let ctx = Context::resolve_in(cwd.to_path_buf(), interrupt)?;
    let open = ctx.open_run(&run_id).await?;
    let (run_dir, manifest) = (open.run_dir, open.manifest.doc);
    let storage = ctx.async_storage().await?;

    // The engine's own door, under this server's injected clock: the
    // round is re-read from the document the node asked from, the reply
    // is judged against it, and either both the acceptance and the
    // `questions_answered` land or neither does.
    yunta_engine::answer_questions(
        &manifest,
        &storage,
        &run_id,
        &run_dir,
        &ctx.clock,
        &node,
        yunta_engine::AnswersReply {
            answers,
            channel: yunta_core::events::Channel::Mcp,
            responder: Some(crate::identity::responder(by.as_ref())),
        },
    )
    .await?;

    // The answers are on the log; a detached process carries the node
    // on from there, exactly as it does after a gate is resolved.
    super::super::spawn_detached_resume(&run_dir, run_id.as_str(), cwd)
        .await
        .map_err(|source| CliError::AnswersRecordedNotResumed {
            source: super::super::DetachedResumeError::new(&run_id, source),
        })?;
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
