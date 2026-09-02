//! Project/config resolution for the CLI.
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
    /// A layer declaring a `version:` this binary doesn't read is
    /// refused up front — parsing on regardless could silently misread
    /// a future format.
    #[error(
        "config layer `{path}` declares `version: {declared}` but this binary reads \
         version {supported}"
    )]
    UnsupportedVersion {
        path: PathBuf,
        declared: u32,
        supported: u32,
    },
}

fn user_root() -> Result<PathBuf, ProjectError> {
    yunta_core::user_state_root().ok_or(ProjectError::NoStateRoot)
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
    let mut layer: ConfigLayer =
        yunta_core::yaml::parse(&contents).map_err(|e| ProjectError::Parse {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    layer
        .expand_home(home.as_deref())
        .map_err(|e| ProjectError::Parse {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;
    if let Some(declared) = layer.version {
        if declared != CONFIG_VERSION {
            return Err(ProjectError::UnsupportedVersion {
                path: path.to_path_buf(),
                declared,
                supported: CONFIG_VERSION,
            });
        }
    }
    Ok(Some(layer))
}

/// The project's config layers as actually present on disk, org first —
/// named so `permissions` conflicts can cite which layer tried to loosen
/// which. Shared by [`resolve`] and by `yunta check`'s layered path.
pub fn load_named_layers(cwd: &Path) -> Result<Vec<(&'static str, ConfigLayer)>, ProjectError> {
    let user_root = user_root()?;
    let candidates = [
        ("org", org_config_path()),
        ("user", user_root.join("config.yaml")),
        ("repo", cwd.join(".yunta/config.yaml")),
    ];

    let mut layers = Vec::new();
    for (name, path) in candidates {
        if let Some(layer) = load_layer(&path)? {
            layers.push((name, layer));
        }
    }
    Ok(layers)
}

/// `version:` a layer may declare — the one this binary reads.
const CONFIG_VERSION: u32 = 1;

/// Finds an existing run's directory, in search order: (a) the current
/// config's runs root, (b) the built-in default under the user state
/// root. Once the manifest inside is open, everything else reads its
/// *frozen* paths — this search only exists because finding the
/// manifest needs somewhere to look first. A run created under roots
/// that no longer appear in any layer needs `YUNTA_HOME` pointing there
/// — a documented limit: a global index would make derived state the
/// source of truth, which this design avoids.
pub fn find_run_dir(project: &Project, run_id: &str) -> Option<PathBuf> {
    let mut candidates = vec![project.runs_root.clone()];
    if let Ok(user_root) = user_root() {
        candidates.push(user_root.join("runs"));
    }
    candidates
        .into_iter()
        .map(|root| root.join(run_id))
        .find(|run_dir| run_dir.join("manifest.yaml").exists())
}

/// Resolves the merged config and state paths for a project rooted at
/// `cwd`. Missing layers are simply absent — an empty config is valid;
/// a malformed one is an error, never silently skipped.
pub fn resolve(cwd: &Path) -> Result<Project, ProjectError> {
    let user_root = user_root()?;

    // merge_layers folds most-specific-last, so feed org → user → repo.
    let layers = load_named_layers(cwd)?.into_iter().map(|(_, layer)| layer);
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
