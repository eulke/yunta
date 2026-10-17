//! The commits a `GitHubForge` makes to the branch a run pushes: the
//! branch itself, then each file of a gate, written only where its content
//! differs from what the branch already holds.

use base64::Engine;
use serde::Deserialize;
use yunta_core::git_blob_id;
use yunta_core::port::ForgeError;

use super::GitHubForge;

impl GitHubForge {
    pub(super) async fn base_branch_sha(&self, base_branch: &str) -> Result<String, ForgeError> {
        #[derive(Deserialize)]
        struct RefObject {
            object: RefSha,
        }
        #[derive(Deserialize)]
        struct RefSha {
            sha: String,
        }
        let parsed: RefObject = self
            .read(
                "resolve the base branch",
                self.request(
                    reqwest::Method::GET,
                    &self.repo_path(&format!("/git/ref/heads/{base_branch}")),
                ),
            )
            .await?;
        Ok(parsed.object.sha)
    }

    /// Idempotent: GitHub answers 422 when the ref already exists, which
    /// is exactly the "already published, `resume` is calling this
    /// again" case — not an error here.
    pub(super) async fn ensure_branch(
        &self,
        branch: &str,
        base_sha: &str,
    ) -> Result<(), ForgeError> {
        let request = self
            .request(reqwest::Method::POST, &self.repo_path("/git/refs"))
            .json(&serde_json::json!({
                "ref": format!("refs/heads/{branch}"),
                "sha": base_sha,
            }));
        match self.send("create the branch", request).await {
            Ok(_) | Err(ForgeError::Http { status: 422, .. }) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// GitHub's Contents API needs the existing blob's `sha` to update a
    /// file that's already there (409 without it) but must omit it to
    /// create a new one — so this looks the file up first. Only a 404
    /// means "no such file"; any other refusal is reported as it is.
    async fn existing_file(
        &self,
        path: &str,
        branch: &str,
    ) -> Result<Option<ExistingFile>, ForgeError> {
        #[derive(Deserialize)]
        struct Contents {
            sha: String,
        }
        let action = "read an artifact's current version";
        let request = self.request(
            reqwest::Method::GET,
            &self.repo_path(&format!("/contents/{path}?ref={branch}")),
        );
        match self.send(action, request).await {
            Ok(response) => {
                let found: Contents =
                    response
                        .json()
                        .await
                        .map_err(|source| ForgeError::Response {
                            action,
                            source: Box::new(source),
                        })?;
                Ok(Some(ExistingFile { sha: found.sha }))
            }
            Err(ForgeError::Http { status: 404, .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Commits `content` to `path` on `branch` — unless the file already
    /// holds it: a commit that changes nothing would move the branch's
    /// head, and a review of it would no longer cover what it reviewed.
    pub(super) async fn commit_artifact(
        &self,
        path: &str,
        content: &[u8],
        branch: &str,
        message: &str,
    ) -> Result<(), ForgeError> {
        let existing = self.existing_file(path, branch).await?;
        // The forge reports the blob id git gives the bytes it holds, so
        // equal ids are equal bytes at any size, even where it sends none.
        if existing
            .as_ref()
            .is_some_and(|file| file.sha == git_blob_id(content))
        {
            return Ok(());
        }
        let mut body = serde_json::json!({
            "message": message,
            "content": base64::engine::general_purpose::STANDARD.encode(content),
            "branch": branch,
        });
        if let (Some(file), Some(object)) = (existing, body.as_object_mut()) {
            object.insert("sha".to_string(), serde_json::Value::String(file.sha));
        }
        let request = self
            .request(
                reqwest::Method::PUT,
                &self.repo_path(&format!("/contents/{path}")),
            )
            .json(&body);
        self.send("commit an artifact", request).await?;
        Ok(())
    }
}

/// A file a branch already holds: the blob id an update names, which also
/// tells whether the branch holds given bytes.
struct ExistingFile {
    sha: String,
}
