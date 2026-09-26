//! Template rendering — `{{name}}` substitution with a hard error on any
//! variable the caller did not define (never silent pass-through: a
//! prompt that ships `{{run.dir}}` verbatim to an agent is a degradation
//! nobody declared).
//!
//! The set of variables is closed, and [`TemplateVar`] is where it is
//! written down: one variant per variable, one spelling per variant, so
//! a surface that renders and a surface that documents cannot disagree
//! about what `{{…}}` may say. A name outside the set is refused where
//! the template is read, naming what it could have been.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::ids::InputName;

/// Every variable a `{{…}}` may name.
///
/// What each one is worth is the renderer's to say — the engine fills a
/// node's, a mock fixture fills a scripted session's — but what they are
/// called is decided here and nowhere else.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemplateVar {
    /// The run's own directory: its log export, its object store, its
    /// scratch.
    RunDir,
    /// The checkout the run's work happens in.
    Worktree,
    /// The branch a run pushes to, derived from the run id.
    RunBranch,
    /// The root every node's staging sits under.
    Staging,
    /// This node's own staging directory — where it writes what it
    /// declares.
    NodeArtifacts,
    /// This node's id, as the workflow declares it.
    NodeId,
    /// The name this node declares under `runner:`, known from the
    /// workflow alone — never the adapter or model a later resolution
    /// picks.
    RunnerName,
    /// The project's name, as the merged config declares it.
    ProjectName,
    /// The branch work merges back into.
    ProjectBaseBranch,
    /// What a run's own branch name starts with.
    ProjectBranchPrefix,
    /// One declared input's resolved value.
    Input(InputName),
}

impl TemplateVar {
    /// Every variable that names no payload, which is every one a
    /// document can list without knowing a workflow.
    pub const FIXED: [TemplateVar; 10] = [
        TemplateVar::RunDir,
        TemplateVar::Worktree,
        TemplateVar::RunBranch,
        TemplateVar::Staging,
        TemplateVar::NodeArtifacts,
        TemplateVar::NodeId,
        TemplateVar::RunnerName,
        TemplateVar::ProjectName,
        TemplateVar::ProjectBaseBranch,
        TemplateVar::ProjectBranchPrefix,
    ];

    /// What an input's variable is spelled under.
    const INPUTS: &'static str = "inputs.";

    /// The variable as a template writes it, braces and all — so a
    /// message that quotes one, and a scan that looks for one, spell it
    /// the same way the renderer reads it.
    pub fn braced(&self) -> String {
        format!("{{{{{self}}}}}")
    }
}

/// The one place a variable's spelling is written down: every other
/// surface renders, parses and quotes it through here.
impl fmt::Display for TemplateVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateVar::RunDir => f.write_str("run.dir"),
            TemplateVar::Worktree => f.write_str("run.worktree"),
            TemplateVar::RunBranch => f.write_str("run.branch"),
            TemplateVar::Staging => f.write_str("run.staging"),
            TemplateVar::NodeArtifacts => f.write_str("node.artifacts"),
            TemplateVar::NodeId => f.write_str("node.id"),
            TemplateVar::RunnerName => f.write_str("runner.name"),
            TemplateVar::ProjectName => f.write_str("project.name"),
            TemplateVar::ProjectBaseBranch => f.write_str("project.base_branch"),
            TemplateVar::ProjectBranchPrefix => f.write_str("project.branch_prefix"),
            TemplateVar::Input(name) => write!(f, "{}{name}", TemplateVar::INPUTS),
        }
    }
}

impl FromStr for TemplateVar {
    type Err = UnknownVariable;

    fn from_str(text: &str) -> Result<Self, UnknownVariable> {
        if let Some(name) = text.strip_prefix(TemplateVar::INPUTS) {
            return InputName::try_from(name.to_string())
                .map(TemplateVar::Input)
                .map_err(|_| UnknownVariable::of(text));
        }
        TemplateVar::FIXED
            .into_iter()
            .find(|variable| variable.to_string() == text)
            .ok_or_else(|| UnknownVariable::of(text))
    }
}

/// A `{{…}}` naming something that is not a variable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "template references `{{{{{name}}}}}`, which is not a variable — the ones that exist are \
         {known} and `inputs.<name>` for each declared input"
)]
pub struct UnknownVariable {
    pub name: String,
    known: String,
}

impl UnknownVariable {
    fn of(name: &str) -> Self {
        UnknownVariable {
            name: name.to_string(),
            known: TemplateVar::FIXED
                .iter()
                .map(|variable| format!("`{variable}`"))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TemplateError {
    #[error("template references `{{{{{name}}}}}`, which is not defined here")]
    Undefined { name: TemplateVar },

    #[error("unclosed `{{{{` at byte {at} — every template must close with `}}}}`")]
    Unclosed { at: usize },

    #[error(transparent)]
    Unknown(#[from] UnknownVariable),
}

/// Replaces every `{{name}}` in `input` with its value from `vars`.
/// Single braces are ordinary text; only the exact `{{ ... }}` form is a
/// template.
pub fn render_template(
    input: &str,
    vars: &BTreeMap<TemplateVar, String>,
) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(input.len());
    for piece in parse(input)? {
        match piece {
            Piece::Text(text) => out.push_str(text),
            Piece::Variable(name) => match vars.get(&name) {
                Some(value) => out.push_str(value),
                None => return Err(TemplateError::Undefined { name }),
            },
        }
    }
    Ok(out)
}

/// Every variable referenced by `input`, in order of appearance — what
/// `yunta check` uses to validate templates statically before any run.
pub fn template_variables(input: &str) -> Result<Vec<TemplateVar>, TemplateError> {
    Ok(parse(input)?
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Variable(name) => Some(name),
            Piece::Text(_) => None,
        })
        .collect())
}

enum Piece<'a> {
    Text(&'a str),
    Variable(TemplateVar),
}

fn parse(input: &str) -> Result<Vec<Piece<'_>>, TemplateError> {
    let mut pieces = Vec::new();
    let mut rest = input;
    let mut offset = 0;

    while let Some(open) = rest.find("{{") {
        if open > 0 {
            pieces.push(Piece::Text(&rest[..open]));
        }
        let after_open = &rest[open + 2..];
        let close = after_open
            .find("}}")
            .ok_or(TemplateError::Unclosed { at: offset + open })?;
        pieces.push(Piece::Variable(after_open[..close].trim().parse()?));

        offset += open + 2 + close + 2;
        rest = &after_open[close + 2..];
    }
    if !rest.is_empty() {
        pieces.push(Piece::Text(rest));
    }
    Ok(pieces)
}
