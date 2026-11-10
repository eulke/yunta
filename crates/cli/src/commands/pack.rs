//! `yunta pack add/new/remove/list/update/audit`: the command layer over the
//! API's packs. It hands the arguments in, shows the audit a person confirms
//! against before anything is vendored, and draws render's `page::pack`; the
//! policy, the vendoring, the lock and the pack's own cases are the API's.
//! The cases run only behind `--run-tests`, after the install; `audit` and
//! `new` run them where they audit and check.

use surco::look::Look;
use yunta_api::Remark;
use yunta_core::PackRef;
use yunta_render::page::pack::{
    Installed, Inventory, Needs, NewPack, NewPackCheck, Removed, Updated,
};

use crate::error::CliError;
use crate::output::{blank, show, Mode, Outcome};
use crate::shell::Shell;

/// `yunta pack add`: the pack fetched, refused before anything is shown by
/// the policy that leaves no decision; its audit shown; installed once a
/// person confirmed what it executes; its cases counted, or run on request;
/// and what its workflows need from the project said.
pub async fn add(
    shell: &Shell,
    source: &str,
    confirmed_executors: bool,
    run_tests: bool,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let stop = shell.stop();
    let say = |remark: Remark| crate::remarks::say(remark, Mode::Human, look);
    let packs = shell.workspace().packs();
    let fetched = packs.fetch(source, confirmed_executors, &stop).await?;
    // The audit is read, never run: the inventory a person confirms against,
    // shown before anything is vendored.
    let audit = fetched.audit();
    let inventory = Inventory::Installing {
        audit: &audit,
        waits_on_yes: fetched.waits_on_yes(),
    };
    show(look, &inventory);
    blank();
    let installed = fetched.install(&say, &stop).await?;
    // The pack's own cases run last, on the vendored copy, and only on
    // request: after the confirmation, never as part of deciding it.
    let cases = match run_tests {
        true => installed.run_cases(&say, &stop).await,
        false => installed.shipped_cases(),
    };
    let needs = installed.needs(&project, &stop).await;
    let at = surco::paths::shown(&installed.dir, &project.workspace.dir, None);
    let page = |needs| Installed {
        manifest: &installed.manifest,
        commit: &installed.commit,
        at: &at,
        cases: &cases,
        needs,
    };
    // The pack is installed whatever stopped the reading of its needs.
    let needs = needs.inspect_err(|_| show(look, &page(Needs::default())))?;
    show(look, &page(needs));
    Ok(Outcome::Success)
}

/// `yunta pack update`: the installed pack fetched at `new_ref`, held to the
/// gates an install meets; its audit shown when it ships executors, since a
/// new ref is where new executor code first appears; then installed.
pub async fn update(
    shell: &Shell,
    pack: &PackRef,
    new_ref: &str,
    confirmed_executors: bool,
    look: &Look,
) -> Result<Outcome, CliError> {
    let stop = shell.stop();
    let packs = shell.workspace().packs();
    let fetched = packs
        .fetch_update(pack, new_ref, confirmed_executors, &stop)
        .await?;
    if !fetched.manifest.declares.executors.is_empty() {
        // A new ref is where new executor code first appears: whoever
        // updates sees the same evidence `add` shows, whatever the policy.
        let audit = fetched.audit();
        let inventory = Inventory::Updating {
            audit: &audit,
            waits_on_yes: fetched.waits_on_yes(),
        };
        show(look, &inventory);
        blank();
    }
    let say = |remark: Remark| crate::remarks::say(remark, Mode::Human, look);
    let installed = fetched.install(&say, &stop).await?;
    let updated = Updated {
        pack,
        to: new_ref,
        commit: &installed.commit,
    };
    show(look, &updated);
    Ok(Outcome::Success)
}

/// `yunta pack remove`: waits while another pack command holds the packs.
pub async fn remove(shell: &Shell, pack: &PackRef, look: &Look) -> Result<Outcome, CliError> {
    let say = |remark: Remark| crate::remarks::say(remark, Mode::Human, look);
    let packs = shell.workspace().packs();
    packs.remove(pack, &say, &shell.stop()).await?;
    show(look, &Removed { pack });
    Ok(Outcome::Success)
}

/// `yunta pack list`.
pub fn list(shell: &Shell, look: &Look) -> Result<Outcome, CliError> {
    let listed = shell.workspace().packs().list()?;
    show(look, &listed);
    Ok(Outcome::Success)
}

/// `yunta pack audit`: the full static inventory of an installed pack (every
/// command, context source, per-node permission, required agent, `mcp`
/// server, executor, repository path a scope names, and each workflow's
/// full, untrimmed prompt), then whether it ships cases of its own and
/// whether they pass. Inventory, never verdict: nothing here flags content
/// as suspicious, it only shows all of it.
pub async fn audit(shell: &Shell, pack: &PackRef, look: &Look) -> Result<Outcome, CliError> {
    let say = |remark: Remark| crate::remarks::say(remark, Mode::Human, look);
    let packs = shell.workspace().packs();
    let audited = packs.audit(pack, &say, &shell.stop()).await?;
    let inventory = Inventory::Audited {
        audit: &audited.audit,
        pack,
        cases: &audited.cases,
    };
    show(look, &inventory);
    Ok(Outcome::Success)
}

/// `yunta pack new`: the skeleton written, its example checked, and, when
/// it checks, its own case run, so a freshly scaffolded pack passes from the
/// first command.
pub async fn new_pack(shell: &Shell, pack: &PackRef, look: &Look) -> Result<Outcome, CliError> {
    let workspace = shell.workspace();
    let scaffolded = workspace.packs().scaffold(pack).await?;
    let at = surco::paths::shown(&scaffolded.dir, &workspace.dir, None);
    let page = |checked| NewPack {
        pack,
        at: &at,
        checked,
    };
    let say = |remark: Remark| crate::remarks::say(remark, Mode::Human, look);
    // The files are on disk whatever stopped the check of them.
    let checked = scaffolded
        .check(&say, &shell.stop())
        .await
        .inspect_err(|_| show(look, &page(None)))?;
    show(look, &page(Some(&checked)));
    Ok(outcome(&checked))
}

/// The exit: reported where the example does not check or a case failed.
fn outcome(checked: &NewPackCheck) -> Outcome {
    match checked {
        NewPackCheck::Tested(cases) if cases.failed() == 0 => Outcome::Success,
        NewPackCheck::Refused { .. } | NewPackCheck::Tested(_) => Outcome::Reported,
    }
}
