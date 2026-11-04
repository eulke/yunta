//! The skill `yunta init` writes for the agents a project runs: when to prefer a verified
//! workflow, and how to find one.

use std::path::Path;

/// Where the skill is written, in a project.
pub(super) const MECHANISM_SKILL_DIR: &str = ".yunta/skills/yunta-mechanism";

/// The mechanism skill: teaches a client agent when to prefer a
/// verified workflow over ad-hoc implementation and how to discover
/// what exists (`yunta list` / `list_workflows`) — the skill itself
/// never embeds a catalog, since the catalog is consulted at the
/// moment it's needed rather than baked in, and one written at `init`
/// time would be empty and stale the moment anyone adds a workflow.
/// Format: a `SKILL.md` with frontmatter, the shape Claude Code's own
/// skill mechanism reads — no other file format is specified anywhere
/// in the docs this was built against.
fn mechanism_skill_content() -> String {
    format!(
        "---\n\
     name: yunta-mechanism\n\
     description: When and how to use Yunta's verified workflows instead of ad-hoc implementation.\n\
     ---\n\n\
     # Using Yunta\n\n\
     This repo uses Yunta, a deterministic workflow engine: a workflow is a \
     declarative DAG whose nodes are mechanically verified (criteria, scope, \
     baseline) before anything is considered done, with a full audit log.\n\n\
     Before implementing something ad hoc, check whether a Yunta workflow \
     already covers it: the catalog is never fixed at install time, so \
     always look it up fresh:\n\n\
     - From a shell: `{list}` (catalog) or `{list_runs}` (local \
       run state).\n\
     - From an MCP-connected client: the `list_workflows` tool.\n\n\
     If a workflow covers the task, prefer `{run}` (or the \
     `run_workflow` MCP tool) over reimplementing it by hand; that path \
     gets mechanical verification and an audit trail this one doesn't. If \
     none covers it, proceed as you normally would; nothing here requires \
     using Yunta for work it has no workflow for.\n\n\
     ## Writing a document Yunta reads\n\n\
     A tasks document, a findings artifact and a questions artifact are \
     validated strictly: a key that is not in the shape fails the node \
     that produced it. Never guess the format; ask for it:\n\n\
     - From a shell: `{schema_kind}`, or `{schemas}` to list the \
       kinds.\n\
     - From an MCP-connected client: the `document_shape` tool.\n",
        list = crate::commands::advice::list(),
        list_runs = crate::commands::advice::list_runs(),
        run = crate::commands::advice::run("<workflow>"),
        schema_kind = crate::commands::advice::schema("<kind>", false),
        schemas = crate::commands::advice::schemas(),
    )
}

pub(super) fn write_mechanism_skill(repo: &Path, force: bool) -> std::io::Result<bool> {
    let dir = repo.join(MECHANISM_SKILL_DIR);
    let path = dir.join("SKILL.md");
    if path.exists() && !force {
        return Ok(false);
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, mechanism_skill_content())?;
    Ok(true)
}
