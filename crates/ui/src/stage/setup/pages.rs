//! See [`super`]. What a setup shows of a repository: the rows of what was found, under the
//! answers given so far, who git commits as and the pack the setup installs as fetching it
//! went, each role on the candidates they write it with, in order, and the row being changed
//! opened in place; the audit of the pack the write installs; and the page of what was
//! written.

use std::path::Path;

use surco::components::Survey;
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::{Committer, InitializedProject, ProjectDraft, ProjectSetup, RunnerDeclaration};
use yunta_core::RunnerName;
use yunta_render::page::pack::Inventory;
use yunta_render::page::{
    Candidates, Choosing, CommitsAs, OfferedPack, Page, Pointed, ProposedCandidate, ProposedRole,
    RepositoryPast, Setup, Typing,
};

use super::rows::{Changing, SetupRow};
use super::Found;
use crate::{PackInstalled, SetupPackFetch};

/// The rows of what was `found`, as the setup's screen lays them out at `look`'s width,
/// naming the `candidates` it has room for: under the name and base branch `setup` stands on
/// and the sentence `past` opens, where it is known; who git commits as, the pack the setup
/// installs once fetching it landed, each role as `setup` writes it; the pointer on the row
/// `pointed` where it stands, and `changing` opened in place there.
pub(super) fn found(
    found: &Found,
    setup: &ProjectSetup,
    past: Option<RepositoryPast>,
    (pointed, changing): (Option<&SetupRow>, Option<&Changing>),
    look: &Look,
    candidates: Candidates,
) -> Survey {
    let draft = found.proposing();
    let roles = proposed_roles(&draft, setup);
    let answering = found.draft.answering();
    let typing = |line: &crate::prompt::TypedLine| {
        let (before, after) = line.split();
        Typing { before, after }
    };
    let typed = match changing {
        Some(Changing::Typed(line)) => Some(typing(line)),
        Some(Changing::Role { .. }) | None => None,
    };
    let choosing = match changing {
        Some(Changing::Role {
            chosen,
            model,
            on_the_model,
        }) => Choosing::among(&answering, *chosen, typing(model), *on_the_model),
        Some(Changing::Typed(_)) | None => None,
    };
    let pointed = pointed.map(|row| match row {
        SetupRow::Name => Pointed::Name(typed),
        SetupRow::Base => Pointed::Base(typed),
        SetupRow::Role(runner) => Pointed::Role(runner, choosing),
        SetupRow::Write => Pointed::Write,
    });
    Setup {
        past,
        name: &setup.name,
        base_branch: &setup.base_branch,
        detected: &draft.detected,
        committer: commits_as(&found.committer),
        pack: found.pack.as_deref().map(offered),
        probed: &draft.probed,
        roles: &roles,
        pointed,
    }
    .survey(look, candidates)
}

/// Who git commits as, as a setup's row says it: by name, or what git said where it names no
/// one.
fn commits_as(committer: &Committer) -> CommitsAs<'_> {
    match committer {
        Committer::Named(who) => CommitsAs::Named(&who.name),
        Committer::Unnamed { git_said } => CommitsAs::Unnamed { git_said },
    }
}

/// The pack a setup installs as its row says it, as `fetched` went.
fn offered(fetched: &SetupPackFetch) -> OfferedPack<'_> {
    match fetched {
        SetupPackFetch::Fetched { pack, .. } => OfferedPack::InReach(pack),
        SetupPackFetch::Installed { pack } => OfferedPack::Installed(pack),
        SetupPackFetch::OutOfReach { pack, why } => OfferedPack::OutOfReach { pack, why },
    }
}

/// Each role `draft` proposes, on the candidates `setup` writes it with, in the order a run
/// tries them: the adapter answered first, with the model typed where one was, then every
/// other adapter this machine answers for; on none where no adapter answers.
fn proposed_roles<'a>(draft: &'a ProjectDraft, setup: &'a ProjectSetup) -> Vec<ProposedRole<'a>> {
    let declared = |runner: &RunnerName| match setup.runners() {
        RunnerDeclaration::Declared(declared) => (declared.iter())
            .find(|declared| &declared.runner == runner)
            .map(|declared| declared.candidates.as_slice()),
        RunnerDeclaration::Proposed => None,
    };
    draft
        .proposed
        .roles()
        .iter()
        .map(|runner| ProposedRole {
            runner,
            candidates: declared(runner)
                .unwrap_or_default()
                .iter()
                .map(|candidate| ProposedCandidate {
                    adapter: &candidate.adapter,
                    model: candidate.model.as_ref(),
                })
                .collect(),
        })
        .collect()
}

/// The audit of the pack `found` was fetched with, as `yunta pack add` shows it before it
/// installs: what it declares, then each workflow, node by node, every prompt whole. Its
/// headline says it is installing, as the setup asks its own confirmation of every install.
/// Nothing where no pack was fetched.
pub(super) fn audited(found: &Found, look: &Look) -> Doc {
    match found.pack.as_deref() {
        Some(SetupPackFetch::Fetched { audit, .. }) => Inventory::Installing {
            audit,
            waits_on_yes: false,
        }
        .document(look),
        Some(SetupPackFetch::Installed { .. } | SetupPackFetch::OutOfReach { .. }) | None => {
            Doc::new()
        }
    }
}

/// The page of what was written into `repo`: where the pack was `installed` with it, the page
/// `yunta pack add` ends with, what the pack still needs from the project among it; the page
/// `init` ends with otherwise.
pub(super) fn written(
    repo: &Path,
    initialized: &InitializedProject,
    installed: Option<&PackInstalled>,
    look: &Look,
) -> Doc {
    match installed {
        Some(installed) => installed.page(look),
        None => initialized.page(repo).document(look),
    }
}
