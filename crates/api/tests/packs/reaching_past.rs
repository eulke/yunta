//! A fetch refuses what could reach past the pack it fetches, before anything of the pack
//! is read: a symbolic link inside the pack's own tree, and a repository or a ref git would
//! read as an option.

use yunta_api::{Error, Stop};
use yunta_core::refusal::{PackRefusal, SourcePart};
use yunta_render::refusal::Refused;
use yunta_testkit::{git, init_repo};

use super::{other_entry, pack_repo, project, write_lock, write_pack};

/// A pack whose declared workflow is a symbolic link to a file outside it is refused as it
/// is fetched, before anything of it is audited or proposed: the file the link reaches is
/// never read.
#[tokio::test]
async fn a_link_inside_a_fetched_pack_is_refused_before_its_audit() {
    let root = tempfile::tempdir().unwrap();
    let secret = root.path().join("secret.txt");
    std::fs::write(&secret, "the text a link reaches\n").unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack(&upstream);
    let workflow = upstream.join("workflows/review.yaml");
    std::fs::remove_file(&workflow).unwrap();
    std::os::unix::fs::symlink(&secret, &workflow).unwrap();
    git(&upstream, &["add", "."]);
    git(&upstream, &["commit", "-q", "-m", "a pack with a link"]);
    let workspace = project(&root.path().join("repo"), root.path());

    let fetched = workspace
        .packs()
        .fetch(upstream.to_str().unwrap(), false, &Stop::new())
        .await;

    match fetched {
        Err(Error::Refused(Refused::Pack(refusal))) => {
            let PackRefusal::LinkInPack { named, link } = *refusal else {
                panic!("refused for another reason: {refusal:?}");
            };
            assert_eq!(named, upstream.display().to_string());
            assert_eq!(
                link, "workflows/review.yaml",
                "the link, by its path in the pack"
            );
        }
        Err(other) => panic!("not refused: {other:?}"),
        Ok(fetched) => panic!("fetched, its audit reading {:?}", fetched.audit()),
    }
}

/// A repository or a ref that begins with `-` would reach git as an option: each is
/// refused before anything is cloned, where a person names a source, where an update
/// names its ref, and where a lock's source is read back for an update.
#[tokio::test]
async fn a_repository_or_a_ref_that_reads_as_an_option_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    pack_repo(&upstream);
    let repo = root.path().join("repo");
    let workspace = project(&repo, root.path());
    let packs = workspace.packs();
    let stop = Stop::new();
    let repository = |written: &str| SourcePart::Repository(written.to_string());
    let reference = |written: &str| SourcePart::Ref(written.to_string());

    let named = [
        ("-u@host:repo".to_string(), repository("-u@host:repo")),
        (
            "--upload-pack=x://host/repo".to_string(),
            repository("--upload-pack=x://host/repo"),
        ),
        (format!("{}@-b", upstream.display()), reference("-b")),
    ];
    for (source, part) in named {
        let fetched = packs.fetch(&source, false, &stop).await;
        assert_eq!(reads_as_an_option(fetched), part, "`{source}`");
    }
    let (pack, mut entry) = other_entry();
    entry.source = "-u@host:repo".to_string();
    write_lock(&repo, &(pack.clone(), entry.clone()));
    let fetched = packs.fetch_update(&pack, "main", false, &stop).await;
    assert_eq!(reads_as_an_option(fetched), repository("-u@host:repo"));
    entry.source = upstream.display().to_string();
    write_lock(&repo, &(pack.clone(), entry));
    let fetched = packs.fetch_update(&pack, "--orphan", false, &stop).await;
    assert_eq!(reads_as_an_option(fetched), reference("--orphan"));
}

/// The part of its source a fetch refused as one git would read as an option, or a panic
/// naming what it was instead.
fn reads_as_an_option(fetched: Result<yunta_api::FetchedPack, Error>) -> SourcePart {
    match fetched {
        Err(Error::Refused(Refused::Pack(refusal))) => match *refusal {
            PackRefusal::ReadsAsAnOption { part, .. } => part,
            other => panic!("refused for another reason: {other:?}"),
        },
        Err(other) => panic!("not refused: {other:?}"),
        Ok(_) => panic!("fetched"),
    }
}
