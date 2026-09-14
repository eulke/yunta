//! The ambient environment a layer resolves against: `$HOME`,
//! `$YUNTA_HOME`, and the `~` expansion of the paths a layer names.

use std::path::PathBuf;

use crate::Secret;

/// Rewrites `path` in place when it starts with `~`: `~` alone becomes
/// `home`, `~/rest` becomes `home/rest`; `~user/...` is refused.
pub(super) fn expand_path(
    path: &mut PathBuf,
    home: Option<&std::path::Path>,
    field: &str,
) -> Result<(), HomeExpansionError> {
    let Some(text) = path.to_str() else {
        return Ok(());
    };
    if !text.starts_with('~') {
        return Ok(());
    }
    let rest = &text[1..];
    if !(rest.is_empty() || rest.starts_with('/')) {
        return Err(HomeExpansionError::OtherUser {
            field: field.to_string(),
            path: text.to_string(),
        });
    }
    let Some(home) = home else {
        return Err(HomeExpansionError::NoHome {
            field: field.to_string(),
            path: text.to_string(),
        });
    };
    *path = home.join(rest.trim_start_matches('/'));
    Ok(())
}

/// The ambient environment a run resolves against, captured once at a
/// shell boundary so no code below the boundary reads the process itself —
/// core stays pure, and a test injects a value instead of mutating the
/// process. A shell (the CLI) fills this from `std::env`; everything else
/// only reads the fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    /// `$HOME` — the user's home directory.
    pub home: Option<PathBuf>,
    /// `$YUNTA_HOME` — the state-root override.
    pub yunta_home: Option<PathBuf>,
    /// `$YUNTA_ORG_CONFIG` — the org config path override.
    pub org_config: Option<PathBuf>,
    /// Variables layered onto every subprocess the run spawns (bash nodes,
    /// hooks, executors) on top of the inherited environment — a test
    /// prepends a stub directory to `PATH` here instead of mutating the
    /// process. Empty in production: subprocesses inherit the run's own
    /// environment unchanged.
    pub subprocess_vars: Vec<(String, String)>,
}

/// The user state root: `$YUNTA_HOME` when set, otherwise `~/.yunta`.
/// `None` only when neither is known. Reads the injected [`Env`], never the
/// process, so the CLI (which layers `config.yaml` from it) and the engine
/// (which reads `knowledge/` from it at context-resolution time) resolve
/// the same "user layer" from one value neither of them read live.
pub fn user_state_root(env: &Env) -> Option<PathBuf> {
    if let Some(root) = &env.yunta_home {
        return Some(root.clone());
    }
    env.home.as_ref().map(|home| home.join(".yunta"))
}

/// A path in a config layer that starts with `~` and cannot be
/// expanded: there is no home directory to expand it against, or the
/// form is one this schema does not read (`~user/...`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HomeExpansionError {
    #[error("`{field}` is `{path}` but no home directory is known — set HOME (or YUNTA_HOME for the state root) or write the path in full")]
    NoHome { field: String, path: String },
    #[error(
        "`{field}` is `{path}` — only `~` and `~/...` expand; write another user's home in full"
    )]
    OtherUser { field: String, path: String },
}

#[cfg(test)]
mod env_tests {
    use super::{user_state_root, Env};
    use std::path::PathBuf;

    #[test]
    fn yunta_home_overrides_home() {
        let env = Env {
            home: Some(PathBuf::from("/home/u")),
            yunta_home: Some(PathBuf::from("/scratch/state")),
            ..Default::default()
        };
        assert_eq!(user_state_root(&env), Some(PathBuf::from("/scratch/state")));
    }

    #[test]
    fn home_falls_back_to_dot_yunta() {
        let env = Env {
            home: Some(PathBuf::from("/home/u")),
            ..Default::default()
        };
        assert_eq!(user_state_root(&env), Some(PathBuf::from("/home/u/.yunta")));
    }

    #[test]
    fn neither_known_is_none() {
        assert_eq!(user_state_root(&Env::default()), None);
    }
}

/// Where a secret's value comes from.
///
/// The config names the variables; the values live only in the process
/// environment of whatever spawned the run, and only the shell that
/// started it may read them. The engine asks here instead, so a test
/// hands it a value it chose and a run can never take one nobody
/// declared.
pub trait SecretSource: Send + Sync {
    /// The value bound to `name`, or `None` when nothing is.
    fn get(&self, name: &str) -> Option<Secret<String>>;
}

/// The real environment of the process the CLI runs in — the one
/// implementation that reads it, so every other module is handed values
/// rather than reaching for them.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessSecrets;

impl SecretSource for ProcessSecrets {
    fn get(&self, name: &str) -> Option<Secret<String>> {
        std::env::var(name).ok().map(Secret::from)
    }
}

/// Every declared secret's value, for keeping them out of the log.
///
/// A secret reaches a session's environment on purpose, and the session
/// may then say it back: a note quoting a command line, an error
/// repeating a URL with a token in it. The log is the run's permanent
/// record and read by whoever reads the run, so what the config named as
/// a secret is taken back out of it on the way in — once, at the one
/// door every event goes through.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    /// Longest first, so a value that contains another is replaced whole
    /// rather than leaving the shorter one's remainder behind.
    values: Vec<String>,
}

/// What stands in the log where a secret was.
pub const REDACTED: &str = "[redacted]";

impl Redactor {
    /// The redactor for the secrets `config` names, as `source` has
    /// them. A name nothing binds contributes nothing: there is no value
    /// to keep out.
    ///
    /// A value shorter than four characters is left alone — `true`, a
    /// one-letter flag, an empty string. Replacing those would blank out
    /// unrelated text everywhere it appeared, which hides more than it
    /// protects.
    pub fn of(names: &[String], source: Option<&dyn SecretSource>) -> Self {
        let Some(source) = source else {
            return Redactor::default();
        };
        let mut values: Vec<String> = names
            .iter()
            .filter_map(|name| source.get(name))
            .map(|secret| secret.expose().to_string())
            .filter(|value| value.chars().count() >= 4)
            .collect();
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        values.dedup();
        Redactor { values }
    }

    /// Whether this redactor has anything to take out.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// `text` with every secret value replaced.
    pub fn text(&self, text: &str) -> String {
        self.values.iter().fold(text.to_string(), |text, value| {
            text.replace(value, REDACTED)
        })
    }

    /// `value` with every secret replaced wherever a string carries one
    /// — at any depth, in a field name as well as a field value, since
    /// neither is a place a secret belongs.
    pub fn json(&self, value: serde_json::Value) -> serde_json::Value {
        use serde_json::Value;
        match value {
            Value::String(text) => Value::String(self.text(&text)),
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|item| self.json(item)).collect())
            }
            Value::Object(fields) => Value::Object(
                fields
                    .into_iter()
                    .map(|(name, field)| (self.text(&name), self.json(field)))
                    .collect(),
            ),
            other => other,
        }
    }
}
