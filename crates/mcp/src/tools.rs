//! See [`crate`]. What each tool the control plane advertises does: one
//! function per tool, each asking the API what the command a person would
//! run asks it, so an agent client and a terminal reach the same verdict.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;
use yunta_api::RunRequest;
use yunta_core::{AdapterId, InputName, ModeName, RunAdapter, RunId};

use crate::error::ToolError;
use crate::ControlPlane;

/// The string argument `name`, refused by name when it is missing or of
/// another type.
fn required_str<'a>(
    args: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<&'a str, ToolError> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Argument(format!("missing or non-string argument `{name}`")))
}

/// An optional string argument: absent or `null` is `None`, and a value of
/// another type is refused by name rather than read as if it were not sent.
fn optional_str<'a>(
    args: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<Option<&'a str>, ToolError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(ToolError::Argument(format!("non-string argument `{name}`"))),
    }
}

/// The `run_id` argument as the id it has to be, so a call naming something
/// that is not one is answered before any disk is read.
fn required_run_id(args: &serde_json::Map<String, Value>) -> Result<RunId, ToolError> {
    required_str(args, "run_id")?
        .parse()
        .map_err(ToolError::invalid)
}

/// The optional argument `name` as the id it has to be.
fn optional_id<T: std::str::FromStr<Err = yunta_core::InvalidId>>(
    args: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<Option<T>, ToolError> {
    optional_str(args, name)?
        .map(str::parse::<T>)
        .transpose()
        .map_err(ToolError::invalid)
}

/// The same catalog `yunta list` draws, read in-process: a command run as a
/// subprocess would print onto the stream the replies travel on. An agent
/// reads it as Markdown: no terminal's width or locale decides what it is
/// told.
pub(crate) async fn list_workflows(plane: &ControlPlane) -> Result<String, ToolError> {
    use surco::surface::{Markdown, Surface};
    use yunta_render::page::Page;
    let catalog = plane.workspace.catalog().await;
    Ok(Markdown.draw(&catalog.document(&Markdown.look())))
}

/// The same versioned document `yunta status --json` prints, written to the
/// tool result and read at the workspace's clock.
pub(crate) async fn workflow_status(
    plane: &ControlPlane,
    args: &serde_json::Map<String, Value>,
) -> Result<String, ToolError> {
    let run_id = required_run_id(args)?;
    let project = plane.workspace.project()?;
    let open = project.run(&run_id).await?;
    let document = open.document(plane.remarks.as_ref()).await?;
    yunta_render::document::to_json_string(&document).map_err(ToolError::Unwritten)
}

/// Resolves the workflow against the whole catalog (the repository's own
/// `.yunta/workflows/`, then a publisher's vendored packs), admits and
/// creates the run as `yunta run --detach` does, and hands it off. The run id
/// first, so a client that reads one line still reads the thing it asked
/// for, and each caution said before the first token under it: a client
/// that starts runs is the one deciding whether the run is worth starting.
pub(crate) async fn run_workflow(
    plane: &ControlPlane,
    args: &serde_json::Map<String, Value>,
) -> Result<String, ToolError> {
    let name = required_str(args, "workflow")?;
    let inputs = inputs(args)?;
    let adapter: Option<AdapterId> = optional_id(args, "adapter")?;
    let mode: Option<ModeName> = optional_id(args, "mode")?;
    let project = plane.workspace.project()?;
    let request = RunRequest {
        workflow: PathBuf::from(name),
        inputs,
        mode,
        adapter: adapter.map(RunAdapter::Named),
        hermetic: false,
    };
    let remarks = plane.remarks.as_ref();
    let admission = project.admit(request, remarks, &plane.stop).await?;
    let created = admission.create(remarks, &plane.stop).await?;
    let warnings = created.warnings.clone();
    let run_id = created.start_detached(&plane.stop).await?;
    Ok(std::iter::once(yunta_render::start::started_id(&run_id))
        .chain(warnings.lines())
        .collect::<Vec<_>>()
        .join("\n"))
}

/// The `inputs` argument: each declared input's name, with its value as the
/// text a person types after `--input name=`.
fn inputs(args: &serde_json::Map<String, Value>) -> Result<HashMap<InputName, String>, ToolError> {
    let object = match args.get("inputs") {
        None | Some(Value::Null) => return Ok(HashMap::new()),
        Some(Value::Object(object)) => object,
        Some(_) => {
            return Err(ToolError::Argument(
                "non-object argument `inputs`".to_string(),
            ))
        }
    };
    let mut inputs = HashMap::new();
    for (key, value) in object {
        let value = typed(value).ok_or_else(|| {
            ToolError::Argument(format!("input `{key}` is `null`, which is no value"))
        })?;
        let name: InputName = key
            .parse()
            .map_err(|error| ToolError::Argument(format!("--input `{key}={value}`: {error}")))?;
        inputs.insert(name, value);
    }
    Ok(inputs)
}

/// Hands a paused run back to a detached resume, refused first when another
/// engine drives it: the process below would say so only to its own log,
/// which no caller of this tool reads. A run paused on a limit's reset is
/// handed to a resume that waits for the reset.
pub(crate) async fn resume_run(
    plane: &ControlPlane,
    args: &serde_json::Map<String, Value>,
) -> Result<String, ToolError> {
    let run_id = required_run_id(args)?;
    let project = plane.workspace.project()?;
    let open = project.run(&run_id).await?;
    open.refuse_if_driven().await?;
    open.resume_detached().await?;
    Ok(yunta_render::start::resumed(&run_id))
}

/// Records the decision and hands the run back, answering with the sentence
/// the command that resolves a gate draws: two doors, one decision, one
/// wording.
pub(crate) async fn resolve_gate(
    plane: &ControlPlane,
    args: &serde_json::Map<String, Value>,
) -> Result<String, ToolError> {
    let run_id = required_run_id(args)?;
    let option = required_str(args, "option")?
        .parse::<yunta_core::OptionId>()
        .map_err(ToolError::invalid)?;
    let by: Option<yunta_core::Responder> = optional_id(args, "by")?;
    let text = optional_str(args, "text")?.map(str::to_string);
    let project = plane.workspace.project()?;
    let open = project.run(&run_id).await?;
    open.decide(option.clone(), by.as_ref(), text).await?;
    Ok(yunta_render::page::GateAnswered::sentence(&run_id, &option))
}

/// Records a reply to the questions a node asked and hands the run on: the
/// round is judged against the document it was asked from, once, inside the
/// API, so this surface and the console reach the same verdict.
pub(crate) async fn answer_questions(
    plane: &ControlPlane,
    args: &serde_json::Map<String, Value>,
) -> Result<String, ToolError> {
    let run_id = required_run_id(args)?;
    let node = required_str(args, "node")?
        .parse::<yunta_core::NodeId>()
        .map_err(ToolError::invalid)?;
    let by: Option<yunta_core::Responder> = optional_id(args, "by")?;
    let answers = answers_from(args)?;
    let project = plane.workspace.project()?;
    let open = project.run(&run_id).await?;
    let reply = yunta_core::port::QuestionsReply {
        answers,
        channel: yunta_core::events::Channel::Mcp,
        responder: by,
    };
    open.answer(&node, reply).await?;
    Ok(yunta_render::page::QuestionsAnswered::sentence(
        &run_id, &node,
    ))
}

/// The `answers` argument as the API takes it: one entry per question, each
/// naming the question it answers.
///
/// Only the shape of the argument is checked here: that it is a list of
/// `{id, value}` with an id that is one. Whether those answers satisfy the
/// questions is the document's own judgement, made once inside the API.
fn answers_from(
    args: &serde_json::Map<String, Value>,
) -> Result<Vec<yunta_core::Answer>, ToolError> {
    let Some(entries) = args.get("answers").and_then(Value::as_array) else {
        return Err(ToolError::Argument(
            "`answers` is required: a list of `{id, value}`, one per question".to_string(),
        ));
    };
    entries
        .iter()
        .map(|entry| {
            let id = entry.get("id").and_then(Value::as_str).ok_or_else(|| {
                ToolError::Argument("every `answers` entry names the `id` it answers".to_string())
            })?;
            let id = id
                .parse::<yunta_core::QuestionId>()
                .map_err(ToolError::invalid)?;
            let value = match entry.get("value").map(typed) {
                Some(Some(value)) => value,
                Some(None) => {
                    return Err(ToolError::Argument(format!(
                        "answer `{id}` is `null`, which is no value"
                    )))
                }
                None => {
                    return Err(ToolError::Argument(format!(
                        "answer `{id}` names no `value`"
                    )))
                }
            };
            Ok(yunta_core::Answer { id, value })
        })
        .collect()
}

/// A value an agent sent, as the text a person types for it: a string as it
/// is, anything else as the JSON it is written in, so a `3` and a "3" are one
/// value. A `null` is no text at all.
fn typed(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn arguments(value: Value) -> serde_json::Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    /// A `null` is no text a person types: an input sent as one is refused by
    /// its name, never recorded as the word `null`.
    #[test]
    fn a_null_input_is_refused_by_its_name() {
        let refused = inputs(&arguments(json!({"inputs": {"topic": null}}))).unwrap_err();
        assert!(refused.said().contains("`topic`"), "{}", refused.said());
    }

    /// An answer sent as `null` is refused by the question it answers.
    #[test]
    fn a_null_answer_is_refused_by_its_question() {
        let answers = json!({"answers": [{"id": "scope", "value": null}]});
        let refused = answers_from(&arguments(answers)).unwrap_err();
        assert!(refused.said().contains("`scope`"), "{}", refused.said());
    }

    /// Every other value is the text it is written in, so a `3` and a "3"
    /// are one answer.
    #[test]
    fn a_number_and_its_text_are_one_answer() {
        let answers = json!({"answers": [{"id": "a", "value": 3}, {"id": "b", "value": "3"}]});
        let read = answers_from(&arguments(answers)).unwrap();
        assert_eq!(read[0].value, read[1].value);
    }
}
