//! A path as a reader reads it: as short as it can be and still name one
//! file.

use std::path::Path;

/// `path` as a line shows it: relative to `cwd` when it is under it,
/// under `~` when it is under `home`, and whole otherwise.
pub fn shown(path: &Path, cwd: &Path, home: Option<&Path>) -> String {
    if let Ok(inside) = path.strip_prefix(cwd) {
        return match inside.as_os_str().is_empty() {
            true => ".".to_string(),
            false => inside.display().to_string(),
        };
    }
    if let Some(under) = home.and_then(|home| path.strip_prefix(home).ok()) {
        return Path::new("~").join(under).display().to_string();
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_shown_as_short_as_it_names_one_file() {
        let (cwd, home) = (Path::new("/home/ana/repo"), Some(Path::new("/home/ana")));
        let shown = |path: &str| shown(Path::new(path), cwd, home);
        assert_eq!(shown("/home/ana/repo/src/lib.rs"), "src/lib.rs");
        assert_eq!(shown("/home/ana/repo"), ".");
        assert_eq!(shown("/home/ana/.yunta/runs/X"), "~/.yunta/runs/X");
        assert_eq!(shown("/tmp/x"), "/tmp/x");
    }
}
