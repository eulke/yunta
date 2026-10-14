//! The ambient environment a layer resolves against: `$HOME`,
//! `$YUNTA_HOME`, and the `~` expansion of the paths a layer names.

use std::collections::BTreeMap;
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
    /// `$PATH`, split into its directories in order — where every
    /// command the run spawns is looked up, unless `subprocess_vars`
    /// sets a `PATH` of its own.
    pub path: Vec<PathBuf>,
    /// Where a session's hooks reach the engine
    /// (`crate::rail::URL_VAR`, `crate::rail::TOKEN_VAR`). Set only in
    /// the environment an adapter built for its child; every other
    /// invocation sees `None`.
    pub rail: Option<crate::rail::RailEndpoint>,
    /// Variables layered onto every subprocess the run spawns (bash nodes,
    /// hooks, executors) on top of the inherited environment — a test
    /// prepends a stub directory to `PATH` here instead of mutating the
    /// process. Empty in production: subprocesses inherit the run's own
    /// environment unchanged.
    pub subprocess_vars: Vec<(String, String)>,
    /// What the shell had for each variable an adapter reaches its
    /// service by — a proxy, a base URL — which its CLI inherits: what a
    /// probe of that service has to look at to reach the same endpoint.
    pub service: BTreeMap<String, String>,
}

impl Env {
    /// The process environment, read once here, at the shell's edge, so
    /// nothing below reads it live: what core and the engine resolve
    /// against, and the value of every name in `service` that is set.
    pub fn of_process(service: &[&str]) -> Env {
        let var = std::env::var_os;
        Env {
            home: var("HOME").map(PathBuf::from),
            yunta_home: var("YUNTA_HOME").map(PathBuf::from),
            org_config: var("YUNTA_ORG_CONFIG").map(PathBuf::from),
            path: var("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default(),
            rail: var(crate::rail::URL_VAR)
                .zip(var(crate::rail::TOKEN_VAR))
                .and_then(|(url, token)| {
                    Some(crate::rail::RailEndpoint {
                        url: url.into_string().ok()?,
                        token: crate::Secret::new(token.into_string().ok()?),
                    })
                }),
            subprocess_vars: Vec::new(),
            service: service
                .iter()
                .filter_map(|name| {
                    let value = var(name)?.into_string().ok()?;
                    Some((name.to_string(), value))
                })
                .collect(),
        }
    }
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

/// The user layer's knowledge: `knowledge/` under the state root the
/// environment names. `None` when no state root is known.
pub fn user_knowledge_dir(env: &Env) -> Option<PathBuf> {
    user_state_root(env).map(|root| root.join("knowledge"))
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
    #[error(
        "`{field}` is `{path}` — a shared directory is shared by every checkout of a run, so \
         it is written in full or from `~`, never relative to one of them"
    )]
    Relative { field: String, path: String },
}

#[cfg(test)]
mod env_tests {
    use super::{user_knowledge_dir, user_state_root, Env};
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

    #[test]
    fn knowledge_lives_under_the_yunta_home_state_root() {
        let env = Env {
            home: Some(PathBuf::from("/home/u")),
            yunta_home: Some(PathBuf::from("/scratch/state")),
            ..Default::default()
        };
        assert_eq!(
            user_knowledge_dir(&env),
            Some(PathBuf::from("/scratch/state/knowledge"))
        );
    }

    #[test]
    fn knowledge_falls_back_to_dot_yunta_under_home() {
        let env = Env {
            home: Some(PathBuf::from("/home/u")),
            ..Default::default()
        };
        assert_eq!(
            user_knowledge_dir(&env),
            Some(PathBuf::from("/home/u/.yunta/knowledge"))
        );
    }

    #[test]
    fn knowledge_is_none_when_neither_is_known() {
        assert_eq!(user_knowledge_dir(&Env::default()), None);
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

/// The shortest declared secret. A shorter value cannot be kept out of a log
/// without blanking unrelated text wherever it appears, so the run refuses it.
pub const MIN_SECRET_CHARS: usize = 4;

/// A declared secret whose bound value is too short to keep out of a log.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("secret `{name}` is shorter than {} characters", MIN_SECRET_CHARS)]
pub struct ShortSecret {
    pub name: String,
}

impl Redactor {
    /// The redactor for the named secrets as `source` binds them. A name
    /// nothing binds contributes nothing: there is no value to keep out.
    /// A bound value under [`MIN_SECRET_CHARS`] is refused, naming the variable.
    pub fn of(names: &[String], source: &dyn SecretSource) -> Result<Self, ShortSecret> {
        let mut values = Vec::new();
        for name in names {
            let Some(secret) = source.get(name) else {
                continue;
            };
            let value = secret.expose().to_string();
            if value.chars().count() < MIN_SECRET_CHARS {
                return Err(ShortSecret { name: name.clone() });
            }
            values.push(value);
        }
        Ok(Self::of_values(values.into_iter().map(Secret::new)))
    }

    /// The redactor for values the caller declares secret: a session's minted
    /// tokens, which are not declared by name and so carry no length floor. An
    /// empty value is dropped: replacing the empty string would mark every position.
    pub fn of_values(values: impl IntoIterator<Item = Secret<String>>) -> Self {
        let mut values: Vec<String> = values
            .into_iter()
            .map(|v| v.expose().to_string())
            .filter(|value| !value.is_empty())
            .collect();
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        values.dedup();
        Redactor { values }
    }

    /// This redactor with `values` added to what it already takes out.
    pub fn extended(&self, values: impl IntoIterator<Item = Secret<String>>) -> Self {
        Self::of_values(
            self.values
                .iter()
                .map(|value| Secret::new(value.clone()))
                .chain(values),
        )
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

    /// `value` with every secret replaced wherever a string carries one — at
    /// any depth, in a field name as well as a field value, since neither is a
    /// place a secret belongs. Scalars are compared by their text too: a secret
    /// spelled `1234` in a number field becomes a string, which the typed field
    /// refuses to read back.
    pub fn json(&self, value: serde_json::Value) -> serde_json::Value {
        use serde_json::Value;
        match value {
            Value::String(text) => Value::String(self.text(&text)),
            other @ (Value::Number(_) | Value::Bool(_)) => {
                let text = other.to_string();
                let kept = self.text(&text);
                if kept == text {
                    other
                } else {
                    Value::String(kept)
                }
            }
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|item| self.json(item)).collect())
            }
            Value::Object(fields) => Value::Object(
                fields
                    .into_iter()
                    .map(|(name, field)| (self.text(&name), self.json(field)))
                    .collect(),
            ),
            Value::Null => Value::Null,
        }
    }

    /// `bytes` with every secret taken out, byte for byte: output that is not
    /// UTF-8 keeps every other byte unchanged.
    pub fn bytes(&self, bytes: &[u8]) -> Vec<u8> {
        self.values.iter().fold(bytes.to_vec(), |bytes, value| {
            replace_bytes(&bytes, value.as_bytes(), REDACTED.as_bytes())
        })
    }
}

/// Every occurrence of `needle` in `haystack` replaced by `with`; `needle` is
/// never empty.
fn replace_bytes(haystack: &[u8], needle: &[u8], with: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(haystack.len());
    let mut rest = haystack;
    while let Some(at) = rest
        .windows(needle.len())
        .position(|window| window == needle)
    {
        let (before, found) = rest.split_at(at);
        out.extend_from_slice(before);
        out.extend_from_slice(with);
        rest = found.get(needle.len()..).unwrap_or_default();
    }
    out.extend_from_slice(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source that binds every name to the same value.
    struct Fixed(&'static str);
    impl SecretSource for Fixed {
        fn get(&self, _name: &str) -> Option<Secret<String>> {
            Some(Secret::new(self.0.to_string()))
        }
    }

    #[test]
    fn a_secret_spelled_as_a_number_is_taken_out_of_the_number_field() {
        let redactor = Redactor::of(&["PORT".to_string()], &Fixed("1234"))
            .expect("four characters is the shortest a secret may be");
        assert_eq!(
            redactor.json(serde_json::json!({ "pid": 1234 })),
            serde_json::json!({ "pid": REDACTED })
        );
    }

    #[test]
    fn a_secret_under_four_characters_is_refused_by_name() {
        let refused = Redactor::of(&["PORT".to_string()], &Fixed("abc"))
            .expect_err("three characters is shorter than the floor");
        assert_eq!(
            refused,
            ShortSecret {
                name: "PORT".to_string()
            }
        );
    }
}
