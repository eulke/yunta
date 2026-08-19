//! Project/config resolution for the CLI (§2.2 — M-0 cut).
//!
//! Layers, most specific first: `.yunta/config.yaml` in the current
//! repo, then the user root's `config.yaml`, then the org layer
//! (`/etc/yunta/config.yaml`, overridable via `YUNTA_ORG_CONFIG`). The
//! user root is `~/.yunta`, or `$YUNTA_HOME` when set — the state-root
//! override for ephemeral environments; it moves the user config layer
//! and all execution state (runs, event log DB) together.

use std::path::{Path, PathBuf};

use yunta_core::ConfigLayer;

pub struct Project {
    pub config: ConfigLayer,
    pub runs_root: PathBuf,
    pub worktrees_root: PathBuf,
    pub storage_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("failed to read config layer `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config layer `{path}`: {detail}")]
    Parse { path: PathBuf, detail: String },
    #[error("no home directory and no YUNTA_HOME set — cannot place state")]
    NoStateRoot,
    #[error("failed to create state directory `{path}`: {source}")]
    CreateStateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

fn user_root() -> Result<PathBuf, ProjectError> {
    if let Ok(home) = std::env::var("YUNTA_HOME") {
        return Ok(PathBuf::from(home));
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".yunta"))
        .ok_or(ProjectError::NoStateRoot)
}

fn org_config_path() -> PathBuf {
    std::env::var_os("YUNTA_ORG_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/yunta/config.yaml"))
}

fn load_layer(path: &Path) -> Result<Option<ConfigLayer>, ProjectError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ProjectError::Read {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    let layer = serde_yaml::from_str(&contents).map_err(|e| ProjectError::Parse {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    Ok(Some(layer))
}

/// Resolves the merged config and state paths for a project rooted at
/// `cwd`. Missing layers are simply absent — an empty config is valid;
/// a malformed one is an error, never silently skipped.
pub fn resolve(cwd: &Path) -> Result<Project, ProjectError> {
    let user_root = user_root()?;

    // merge_layers folds most-specific-last, so feed org → user → repo.
    let layers = [
        org_config_path(),
        user_root.join("config.yaml"),
        cwd.join(".yunta/config.yaml"),
    ]
    .iter()
    .filter_map(|path| load_layer(path).transpose())
    .collect::<Result<Vec<_>, _>>()?;
    let config = ConfigLayer::merge_layers(layers);

    let runs_root = config
        .paths
        .as_ref()
        .and_then(|paths| paths.runs.clone())
        .unwrap_or_else(|| user_root.join("runs"));
    let worktrees_root = config
        .paths
        .as_ref()
        .and_then(|paths| paths.worktrees.clone())
        .unwrap_or_else(|| user_root.join("worktrees"));
    let storage_path = config
        .storage
        .as_ref()
        .and_then(|storage| storage.path.clone())
        .unwrap_or_else(|| user_root.join("yunta.db"));

    // First use creates the state root — SQLite creates files, never
    // directories.
    if let Some(parent) = storage_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ProjectError::CreateStateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    Ok(Project {
        config,
        runs_root,
        worktrees_root,
        storage_path,
    })
}
