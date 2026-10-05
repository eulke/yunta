//! What one invocation of a run gives every command it starts: the
//! variables layered onto each subprocess, and the shared directories
//! those name, made before anything writes there.

use super::RunError;

/// Every variable layered onto a subprocess of the run: the ambient
/// ones a caller injects, then each shared directory under its variable.
pub(super) fn subprocess_vars(
    ambient: Option<&yunta_core::Env>,
    config: &yunta_core::ConfigLayer,
) -> Vec<(String, String)> {
    let mut vars = ambient
        .map(|ambient| ambient.subprocess_vars.clone())
        .unwrap_or_default();
    vars.extend(
        config
            .shared_dirs()
            .map(|(var, dir)| (var.to_string(), dir.display().to_string())),
    );
    vars
}

/// Makes every shared directory exist before anything is asked to write
/// in it: a session's sandbox keeps a directory writable, it does not
/// create one.
pub(super) async fn share_dirs(config: &yunta_core::ConfigLayer) -> Result<(), RunError> {
    for (_, dir) in config.shared_dirs() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|source| RunError::Io {
                context: format!("create the shared directory `{}`", dir.display()),
                source,
            })?;
    }
    Ok(())
}
