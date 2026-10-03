//! The GitHub repository a checkout's `origin` is.

use std::path::Path;

use yunta_core::GitHubRepo;
use yunta_engine::process::Supervision;

pub(super) async fn github_repo(repo: &Path, supervision: Supervision<'_>) -> Option<GitHubRepo> {
    let url = yunta_engine::git::remote_url(repo, "origin", supervision).await?;
    parse(&url)
}

/// `owner/name` out of an https, ssh or scp-style GitHub URL; `None` for
/// any other host.
fn parse(url: &str) -> Option<GitHubRepo> {
    let path = [
        "https://github.com/",
        "http://github.com/",
        "ssh://git@github.com/",
        "git@github.com:",
    ]
    .iter()
    .find_map(|prefix| url.strip_prefix(prefix))?;
    let path = path.trim_end_matches('/');
    path.strip_suffix(".git").unwrap_or(path).parse().ok()
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn every_github_url_names_its_repository() {
        for url in [
            "https://github.com/acme/web.git",
            "https://github.com/acme/web",
            "git@github.com:acme/web.git",
            "ssh://git@github.com/acme/web.git",
        ] {
            assert_eq!(
                parse(url).map(|repo| repo.to_string()),
                Some("acme/web".to_string()),
                "{url}"
            );
        }
    }

    #[test]
    fn another_host_names_no_github_repository() {
        assert_eq!(parse("https://gitlab.com/acme/web.git"), None);
        assert_eq!(parse("/srv/git/web.git"), None);
    }
}
