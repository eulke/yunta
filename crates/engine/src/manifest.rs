//! Manifest resolution — the IO shell that freezes
//! a [`Manifest`]. Reading prompt files, freezing what the workflow
//! consumes and asking git for the base commit happen here, once, at run
//! creation; everything downstream operates on the frozen value and never
//! goes back to disk.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use thiserror::Error;
use yunta_core::protected::{LoadedLink, Protected, ProtectedError};
use yunta_core::{
    content_hash, CommitSha, ConfigLayer, ContentHash, Frozen, FrozenWorkflow, InputName,
    InvalidId, Manifest, NodeId, PackProvenance, ProvenanceGap, RunAdapter, Workflow,
};

use crate::frozen::prompt::{file_prompt, read_prompts, PromptRoot};
use crate::frozen::provenance::pack_provenance;
use crate::frozen::FrozenError;
use crate::git::GitError;
use crate::inputs::{resolve_inputs, InputsError};
use crate::run::BirthArtifact;

#[derive(Debug, Error)]
pub enum ManifestError {
    /// git could not answer for the base commit: it failed, or the token
    /// stopped it.
    #[error("failed to resolve the base commit: {0}")]
    Git(GitError),
    #[error("the base commit of `{cwd}` is not a commit id: {source}")]
    BaseCommit {
        cwd: PathBuf,
        #[source]
        source: InvalidId,
    },
    #[error(transparent)]
    Inputs(#[from] InputsError),
    /// A frozen input could not be read or written; the error names it.
    #[error(transparent)]
    Frozen(#[from] FrozenError),
    /// git could not list the base commit's links or read what they point
    /// to.
    #[error("failed to read the links the base commit holds: {0}")]
    Links(GitError),
    /// The protected set does not build with what the base commit's links
    /// resolve to.
    #[error(transparent)]
    Protected(#[from] ProtectedError),
}

impl ManifestError {
    /// Whether the token stopped the git this freeze asked.
    pub fn cancelled(&self) -> bool {
        matches!(self, ManifestError::Git(git) | ManifestError::Links(git) if git.cancelled())
    }
}

/// Everything a run is frozen with before it exists: the manifest it
/// carries, and the artifacts it is born holding.
///
/// The two halves travel to different places — the manifest to
/// `manifest.yaml` and `run_created`, the documents to the run's log as
/// acceptances — so they are two fields rather than one value with a
/// part nobody reads. Whoever creates the run hands both to
/// [`create_run`](crate::create_run), which is where the order between
/// them is settled. `blobs` are the bytes of the frozen inputs the
/// manifest names by hash; `create_run` writes them before the manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct FrozenRun {
    pub manifest: Manifest,
    /// One per `document` input, in declaration order.
    pub documents: Vec<BirthArtifact>,
    pub blobs: Vec<(ContentHash, Vec<u8>)>,
}

impl FrozenRun {
    /// Sets the adapter the run is born on: what `yunta run --adapter`
    /// asked for, or what a child inherits from the run that starts it.
    pub fn with_adapter(mut self, adapter: Option<RunAdapter>) -> Self {
        self.manifest.adapter = adapter;
        self
    }
}

/// Freezes a manifest from already-parsed sources, with the documents
/// its `inputs:` named.
///
/// `workflow_dir` is the directory of the workflow file — `prompt:
/// {file: ...}` paths resolve relative to it. `repo` is the
/// working tree whose `HEAD` becomes the base commit, and also the base
/// a `path`- or `document`-typed input in `provided_inputs` resolves
/// against: inputs are validated before any worktree exists, so there is
/// nowhere else for a relative path to mean. A `document` input is read
/// and held to its kind here, before the base commit is even asked for,
/// so a document nobody can use costs no worktree and no baseline.
///
/// `user_knowledge` is the directory of the user's knowledge layer, when
/// the caller has one: what the workflow reads from it is frozen too.
pub async fn build_manifest(
    workflow: &Workflow,
    config: &ConfigLayer,
    workflow_dir: &Path,
    repo: &Path,
    provided_inputs: &HashMap<InputName, String>,
    supervision: crate::process::Supervision<'_>,
    user_knowledge: Option<&Path>,
) -> Result<FrozenRun, ManifestError> {
    birth(
        workflow,
        config,
        repo,
        provided_inputs,
        supervision,
        Origin::Tree {
            workflow_dir,
            user_knowledge,
        },
    )
    .await
}

/// Births a child of a run that froze its inputs, from the copies that run
/// froze: its prompts, its frozen set and the objects behind it. The
/// checkout is asked only for the pack the child's name resolves to.
pub(crate) async fn child_run(
    child: &Workflow,
    config: &ConfigLayer,
    repo: &Path,
    provided_inputs: &HashMap<InputName, String>,
    supervision: crate::process::Supervision<'_>,
    parent: ParentRun<'_>,
) -> Result<FrozenRun, ManifestError> {
    birth(
        child,
        config,
        repo,
        provided_inputs,
        supervision,
        Origin::Parent(parent),
    )
    .await
}

/// What a child run takes from the parent run that starts it: the name the
/// parent's `use:` gives the child, the set the parent was born with, and the
/// run directory that holds that set's objects.
pub(crate) struct ParentRun<'a> {
    pub(crate) use_name: &'a str,
    pub(crate) frozen: &'a Frozen,
    pub(crate) run_dir: &'a Path,
}

/// Where a birth gets its workflow's prompts and frozen set.
enum Origin<'a> {
    /// The tree the run is born in: the prompt files and everything the
    /// workflow reaches are read now.
    Tree {
        workflow_dir: &'a Path,
        user_knowledge: Option<&'a Path>,
    },
    /// The set the parent run was born with, and the run directory its
    /// objects are in.
    Parent(ParentRun<'a>),
}

/// What a birth freezes about its workflow, whichever way it was found.
struct Freezing {
    prompts: BTreeMap<NodeId, String>,
    frozen: Frozen,
    blobs: Vec<(ContentHash, Vec<u8>)>,
    pack: Option<PackProvenance>,
    provenance_gaps: Vec<ProvenanceGap>,
}

/// The birth both entry points share: inputs resolved, the workflow's
/// prompts and frozen set taken from `origin`, and the tree's git facts.
async fn birth(
    workflow: &Workflow,
    config: &ConfigLayer,
    repo: &Path,
    provided_inputs: &HashMap<InputName, String>,
    supervision: crate::process::Supervision<'_>,
    origin: Origin<'_>,
) -> Result<FrozenRun, ManifestError> {
    let mut workflow = workflow.clone();
    yunta_core::workflow::read::expand_runner_fanout(&mut workflow);
    yunta_core::workflow::read::expand_implicit_dependencies(&mut workflow);

    let resolved = resolve_inputs(&workflow.inputs, provided_inputs, repo).await?;
    let freezing = freeze_origin(&workflow, config, repo, origin).await?;

    let (base_commit, base_branch) = base_of(repo, supervision).await?;
    let loaded_links = loaded_links(repo, &base_commit, supervision).await?;
    let manifest = Manifest {
        schema_version: <Manifest as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
        yunta_version: env!("CARGO_PKG_VERSION").to_string(),
        workflow_hash: content_hash(&workflow),
        config_hash: content_hash(config),
        workflow,
        config: config.clone(),
        inputs: resolved.values,
        prompts: freezing.prompts,
        base_branch,
        base_commit,
        isolation: config.resolved_isolation(),
        max_parallel_nodes: config.resolved_max_parallel_nodes(),
        // The resolved state roots live in the shell that knows them
        // (the CLI's project resolution) — it fills this in before
        // `create_run` freezes the manifest. `None` here keeps
        // library callers (tests) on the fallback-to-current-config
        // path, which is also the tolerant reading of old manifests.
        paths: None,
        pack: freezing.pack,
        project: project_identity(repo, supervision).await?,
        // `yunta run --adapter` sets it, as the CLI sets `paths`.
        adapter: None,
        frozen: Some(freezing.frozen),
        provenance_gaps: freezing.provenance_gaps,
        loaded_links,
    };
    Ok(FrozenRun {
        manifest,
        documents: resolved.documents,
        blobs: freezing.blobs,
    })
}

async fn freeze_origin(
    workflow: &Workflow,
    config: &ConfigLayer,
    repo: &Path,
    origin: Origin<'_>,
) -> Result<Freezing, ManifestError> {
    match origin {
        Origin::Tree {
            workflow_dir,
            user_knowledge,
        } => freeze_tree(workflow, config, repo, workflow_dir, user_knowledge).await,
        Origin::Parent(ParentRun {
            use_name,
            frozen,
            run_dir,
        }) => inherit_frozen(workflow, use_name, frozen, run_dir).await,
    }
}

/// Reads the workflow's own prompt files from `workflow_dir`, and freezes
/// everything the tree holds for the workflow's children.
async fn freeze_tree(
    workflow: &Workflow,
    config: &ConfigLayer,
    repo: &Path,
    workflow_dir: &Path,
    user_knowledge: Option<&Path>,
) -> Result<Freezing, ManifestError> {
    let root = PromptRoot::of_tree(repo, workflow_dir).await;
    let prompts = read_prompts(workflow, &root).await?;
    let (frozen, blobs) =
        crate::frozen::walk::freeze(repo, workflow, config, user_knowledge).await?;
    let (pack, provenance_gaps) = pack_provenance(repo, workflow_dir).await;
    Ok(Freezing {
        prompts,
        frozen,
        blobs,
        pack,
        provenance_gaps,
    })
}

/// Takes a child's text, prompts and pack provenance, and the set its parent
/// was born with, from the parent's frozen copy of it.
async fn inherit_frozen(
    workflow: &Workflow,
    use_name: &str,
    frozen: &Frozen,
    run_dir: &Path,
) -> Result<Freezing, ManifestError> {
    let child = frozen.workflows.get(use_name).ok_or_else(|| {
        FrozenError::Unresolved(format!(
            "child workflow `use: {use_name}` cannot be resolved: the run froze no such workflow"
        ))
    })?;
    let prompts = inherited_prompts(workflow, use_name, child)?;
    let blobs = crate::frozen::store::inherit(run_dir, frozen).await?;
    Ok(Freezing {
        prompts,
        frozen: frozen.clone(),
        blobs,
        pack: child.pack.clone(),
        provenance_gaps: child.provenance_gaps.clone(),
    })
}

/// The prompt each file prompt of `workflow` reads, as the parent froze it;
/// a file prompt with no frozen text cannot run its node.
fn inherited_prompts(
    workflow: &Workflow,
    use_name: &str,
    frozen: &FrozenWorkflow,
) -> Result<BTreeMap<NodeId, String>, FrozenError> {
    let mut prompts = BTreeMap::new();
    for node in workflow
        .iter_nodes()
        .filter(|node| file_prompt(node).is_some())
    {
        let text = frozen
            .prompts
            .get(&node.id)
            .ok_or_else(|| FrozenError::Missing {
                input: format!("the prompt of node `{}` in `use: {use_name}`", node.id),
            })?;
        prompts.insert(node.id.clone(), text.clone());
    }
    Ok(prompts)
}

/// The commit `repo`'s `HEAD` stands at and the branch it names
/// (`"HEAD"` when detached): where a run of it starts from.
async fn base_of(
    repo: &Path,
    supervision: crate::process::Supervision<'_>,
) -> Result<(CommitSha, String), ManifestError> {
    let commit = git_line(repo, &["rev-parse", "HEAD"], supervision)
        .await?
        .parse()
        .map_err(|source| ManifestError::BaseCommit {
            cwd: repo.to_path_buf(),
            source,
        })?;
    let branch = git_line(repo, &["rev-parse", "--abbrev-ref", "HEAD"], supervision).await?;
    Ok((commit, branch))
}

/// The links `base` holds where an agent CLI loads its configuration or its
/// instructions, each resolved inside the checkout: one listing of the
/// commit's files, and one read of every link's text when it holds a link.
/// The read never fetches: a partial clone that lacks a link's text answers
/// it missing, and that link resolves to nothing.
pub(crate) async fn loaded_links(
    repo: &Path,
    base: &CommitSha,
    supervision: crate::process::Supervision<'_>,
) -> Result<Vec<LoadedLink>, ManifestError> {
    let listing = ["ls-tree", "-r", "-z", "--full-tree", base.as_str()];
    let listed = crate::git::read_bytes(repo, &listing, supervision)
        .await
        .map_err(ManifestError::Links)?;
    let mut files = HashSet::new();
    let mut found = Vec::new();
    for (mode, object, path) in tree_entries(&listed) {
        match mode {
            "120000" => found.push((path, object)),
            "160000" => {}
            _ => {
                files.insert(path);
            }
        }
    }
    let links = link_texts(repo, found, supervision).await?;
    let is_file = |path: &Path| path.to_str().is_some_and(|path| files.contains(path));
    let fixed = Protected::new(&[], &[])?;
    Ok(fixed.loaded(yunta_core::protected::resolved(&links, is_file))?)
}

/// The text each of `found`'s links points by, keyed by the link's path: one
/// `cat-file --batch` read, which never fetches an object the clone lacks.
async fn link_texts(
    repo: &Path,
    found: Vec<(&str, &str)>,
    supervision: crate::process::Supervision<'_>,
) -> Result<BTreeMap<PathBuf, String>, ManifestError> {
    if found.is_empty() {
        return Ok(BTreeMap::new());
    }
    let asked: String = found
        .iter()
        .map(|(_, object)| format!("{object}\n"))
        .collect();
    let mut env = supervision.env.to_vec();
    env.push(("GIT_NO_LAZY_FETCH".to_string(), "1".to_string()));
    let batch = ["cat-file", "--batch"];
    let answered =
        crate::git::read_batch(repo, &batch, asked.into_bytes(), supervision.with_env(&env))
            .await
            .map_err(ManifestError::Links)?;
    Ok(found
        .into_iter()
        .zip(batch_contents(&answered))
        .map(|((path, _), text)| (PathBuf::from(path), text))
        .collect())
}

/// The entries of an `ls-tree -z` listing: each one's mode, object and path.
/// An entry whose path is not UTF-8 is left out: the manifest keeps a link's
/// paths as text, so a link there, or a file it resolves to, is not followed.
fn tree_entries(listed: &[u8]) -> impl Iterator<Item = (&str, &str, &str)> {
    listed
        .split(|byte| *byte == 0)
        .filter_map(|entry| std::str::from_utf8(entry).ok())
        .filter_map(|entry| {
            let (fields, path) = entry.split_once('\t')?;
            let mut fields = fields.split(' ');
            let mode = fields.next()?;
            let object = fields.nth(1)?;
            Some((mode, object, path))
        })
}

/// What `cat-file --batch` answered, one text per object asked and in the
/// order asked. An object it reports missing reads as an empty text, which
/// resolves to nothing.
fn batch_contents(answered: &[u8]) -> Vec<String> {
    let mut contents = Vec::new();
    let mut rest = answered;
    while let Some((header, after)) = first_line(rest) {
        let size = String::from_utf8_lossy(header)
            .rsplit(' ')
            .next()
            .and_then(|size| size.parse::<usize>().ok());
        match size.and_then(|size| after.split_at_checked(size)) {
            Some((text, past)) => {
                contents.push(String::from_utf8_lossy(text).into_owned());
                rest = past.strip_prefix(b"\n").unwrap_or(past);
            }
            None => {
                contents.push(String::new());
                rest = after;
            }
        }
    }
    contents
}

/// The bytes before the first newline, and those after it.
fn first_line(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let end = bytes.iter().position(|byte| *byte == b'\n')?;
    let (line, rest) = bytes.split_at_checked(end)?;
    Some((line, rest.get(1..).unwrap_or_default()))
}

/// The git directory the repository's checkouts share, which names the
/// project. A git that cannot name it leaves the project unknown; a git the
/// token stopped stops the freeze, as every other git the run asks does.
async fn project_identity(
    repo: &Path,
    supervision: crate::process::Supervision<'_>,
) -> Result<Option<yunta_core::ProjectIdentity>, ManifestError> {
    match crate::git::common_dir(repo, supervision).await {
        Ok(git_common_dir) => Ok(Some(yunta_core::ProjectIdentity { git_common_dir })),
        Err(error) if error.cancelled() => Err(ManifestError::Git(error)),
        Err(_) => Ok(None),
    }
}

async fn git_line(
    repo: &Path,
    args: &[&str],
    supervision: crate::process::Supervision<'_>,
) -> Result<String, ManifestError> {
    crate::git::read(repo, args, supervision)
        .await
        .map(|stdout| stdout.trim().to_string())
        .map_err(ManifestError::Git)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_answer_reads_one_text_per_object_and_a_missing_one_as_empty() {
        let answered = b"a1 blob 4\n../x\nb2 missing\nc3 blob 5\na\nb/c\n";
        assert_eq!(batch_contents(answered), ["../x", "", "a\nb/c"]);
    }

    /// A token that stopped the read of the links is the birth being
    /// cancelled, not a failure of the base commit.
    #[tokio::test]
    async fn a_stopped_read_of_the_links_is_a_cancellation() {
        let repo = tempfile::tempdir().unwrap();
        yunta_testkit::init_repo(repo.path());
        let head = yunta_testkit::git_output(repo.path(), &["rev-parse", "HEAD"]);
        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();
        let clock = yunta_testkit_core::FixedClock;
        let supervision = crate::process::Supervision::outside_any_run(&token, &clock);
        let error = loaded_links(repo.path(), &head.parse().unwrap(), supervision)
            .await
            .expect_err("a stopped git answers nothing");
        assert!(matches!(error, ManifestError::Links(_)), "{error}");
        assert!(error.cancelled(), "{error}");
    }
}
