//! Template rendering — `{{name}}` substitution with a hard error on
//! any undefined variable (never silent pass-through: a prompt that
//! ships `{{run.dir}}` verbatim to an agent is a degradation nobody
//! declared). The syntax here is the full, final grammar; growing the
//! available variables (`{{inputs.*}}`, `{{runner.role}}`, ...) never
//! requires touching the syntax itself.

use std::collections::BTreeMap;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TemplateError {
    #[error("template references `{{{{{name}}}}}`, which is not defined here")]
    Undefined { name: String },

    #[error("unclosed `{{{{` at byte {at} — every template must close with `}}}}`")]
    Unclosed { at: usize },
}

/// Replaces every `{{name}}` in `input` with its value from `vars`.
/// Single braces are ordinary text; only the exact `{{ ... }}` form is a
/// template.
pub fn render_template(
    input: &str,
    vars: &BTreeMap<String, String>,
) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(input.len());
    for piece in parse(input)? {
        match piece {
            Piece::Text(text) => out.push_str(text),
            Piece::Variable(name) => match vars.get(name) {
                Some(value) => out.push_str(value),
                None => {
                    return Err(TemplateError::Undefined {
                        name: name.to_string(),
                    })
                }
            },
        }
    }
    Ok(out)
}

/// Every variable referenced by `input`, in order of appearance — what
/// `yunta check` uses to validate templates statically before any run.
pub fn template_variables(input: &str) -> Result<Vec<String>, TemplateError> {
    Ok(parse(input)?
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Variable(name) => Some(name.to_string()),
            Piece::Text(_) => None,
        })
        .collect())
}

enum Piece<'a> {
    Text(&'a str),
    Variable(&'a str),
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
        pieces.push(Piece::Variable(after_open[..close].trim()));

        offset += open + 2 + close + 2;
        rest = &after_open[close + 2..];
    }
    if !rest.is_empty() {
        pieces.push(Piece::Text(rest));
    }
    Ok(pieces)
}
