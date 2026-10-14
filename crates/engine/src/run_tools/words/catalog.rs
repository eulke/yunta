//! What each run tool is described as, and what its schema says of its
//! arguments: the words a session reads when it is offered a tool. A
//! description names the other tools it points at as the caller gives them,
//! so each sentence names them the way that session's CLI does.

use yunta_core::ArtifactKind;

/// What the check tool does: it judges an artifact the node declares.
pub(crate) fn check_artifact_description() -> &'static str {
    "Check an artifact this node declares, before your session ends: for a \
     file you wrote, that it is there and within its size; for a document you \
     submitted, what the engine read out of the file it wrote. Runs exactly the \
     verification the node's close runs, so a clean answer here is a clean \
     close. Omit `name` to check every artifact this node declares."
}

/// The `name` argument of the check tool: the artifact as the node declares it.
pub(crate) fn artifact_name_description() -> &'static str {
    "The artifact, as the node declares it: a kind for a document the engine \
     reads, a file name otherwise. Omit to check them all."
}

/// What the tool that posts a finding does, and the tools that change or
/// withdraw one.
pub(crate) fn post_finding_description(update: &str, withdraw: &str) -> String {
    format!(
        "Report a structured finding the moment you see it — one call per \
         finding. The engine validates it at once: unknown keys, wrong types, \
         empty fields, and an id this node already used are refused with what \
         to fix, and nothing else you reported is lost. An accepted finding is \
         counted, deduplicated and consulted with every other finding on the \
         run, survives this session, and — when this node declares a `findings` \
         artifact — is written into that file at the end. Never write a findings \
         file yourself. A `proposed_criterion` is a command that fails on the run's \
         tree now and passes once the finding is fixed: one that already passes, or \
         cannot run, is refused. To change a finding you reported, use `{update}`; to \
         take one back, `{withdraw}`."
    )
}

/// What the tool that replaces a finding does, and the tool that posts one.
pub(crate) fn update_finding_description(post: &str) -> String {
    format!(
        "Replace a finding this node already reported, by id, with its whole \
         new content — same fields as `{post}`, validated the same way. Use it when \
         a finding turns out to be more or less severe, wrongly located, or better \
         explained. Only a finding this node reported can be updated, and a \
         withdrawn one cannot; the previous state stays in the run's log."
    )
}

/// What the tool that takes a finding back does.
pub(crate) fn withdraw_finding_description() -> &'static str {
    "Take back a finding this node reported, by id, saying why — a non-empty \
     `reason`, such as a false positive or something fixed by other work. A \
     withdrawn finding is left out of every count, file and view from now on, \
     but the log keeps it and the reason. Withdrawal is final: a finding that \
     comes back is a new id."
}

/// What the tool that reads the findings standing in the run does, and the
/// tool that answers one.
pub(crate) fn findings_description(answer: &str) -> String {
    format!(
        "Read the findings standing in this run: each with the `node` that reported it — \
         none for one the engine reported about the run itself — its id, severity, title, \
         location, detail and proposed criterion, and the `answers` other nodes gave it \
         with `{answer}`. A finding taken back is not here. What a gate shows a person \
         about the run's findings is this same view."
    )
}

/// What the tool that answers another node's finding does, and the tools
/// that change, withdraw or read findings.
pub(crate) fn answer_finding_description(update: &str, withdraw: &str, findings: &str) -> String {
    format!(
        "Answer a finding another node reported: `fixed` when your work fixed it, \
         `declined` when it is wrong or its fix belongs to another change — always saying \
         `why`. Name it by the node that reported it and its id, as `{findings}` shows \
         them. The answer stands beside \
         the finding for a person to read, and answering it again replaces what you \
         answered before. A finding your own node reported is yours to change with \
         `{update}` or take back with `{withdraw}`."
    )
}

/// The `node` argument of the answer tool: the node that reported the finding.
pub(crate) fn answer_node_description() -> &'static str {
    "The node that reported the finding."
}

/// The `id` argument of the answer tool: the finding's id, as its node gave it.
pub(crate) fn answer_id_description() -> &'static str {
    "The finding's id, as its node reported it."
}

/// What the tool that reads the run's tasks document by status does.
pub(crate) fn task_status_description() -> &'static str {
    "Read-only view of the run's tasks document (task id -> status) — the same data \
     the `tasks` context source mounts, queryable mid-session."
}

/// What the tool that reads this session's task does.
pub(crate) fn task_description() -> &'static str {
    "Read the task this session works, from the run's tasks document: its id, title \
     and notes; `scope`, the globs every change must stay inside (what the task declared \
     plus what was granted to it); `denied`, what no change may touch whatever the scope \
     — what the project denies, and the files of every test a person approved, this \
     task's own among them; and `criteria`, the commands that must all exit 0 \
     when your session ends — a criterion is red before the work starts, and a `guard` \
     is green before it and must stay green; each may say what it `proves`. When the run \
     measured its suite green before changing anything, that suite is among the guards: \
     a change that breaks what passed keeps the task open. `cycles` lists what the engine \
     found each time the task ran, the current cycle last: its pre-check, then each \
     attempt's criteria and the paths it changed outside the scope. `plan` is the plan \
     the task belongs to: its summary, description and `design` — the shapes it creates, \
     which your task names rather than restates — its risks, what it leaves out, and \
     `other_tasks`, which own what your scope leaves out. The tasks document is not \
     in your checkout; this is where it is read."
}

/// What the tool that judges this session's work on its task does.
pub(crate) fn check_task_description() -> &'static str {
    "Judge your work on this task exactly as the engine will when your session \
     ends: run the task's criteria on the checkout as it stands and audit what \
     changed against the task's scope. Its guards run once every other criterion \
     passes, in order and stopping at the first that fails; `guards_waiting` names \
     those not run yet. `closes` is true when every \
     criterion and guard exits 0 and nothing changed lies outside the scope — the \
     task is then done if the tree does not change again. A scope expansion you \
     asked for counts only once granted. \
     It answers when the criteria have, however long they take. Calling it again on \
     an unchanged checkout waits for the check already running and reuses its \
     answers; calling it after changing the checkout stops that check, whose tree \
     nobody will close any more."
}

/// What the tool that audits a node's scope does, and the tool that asks for
/// more scope.
pub(crate) fn check_scope_description(ask: &str) -> String {
    format!(
        "Audit what this node changed against its scope exactly as the engine will \
         when your session ends. `scope` is what you may write — what the node \
         declared plus what a person granted it — and `outside_scope` lists every \
         path you changed outside it. A path outside fails the node; if your fix \
         needs one, ask with `{ask}` instead of writing it — unless no session of \
         the run may write it (what the project denies, or a test a person \
         approved), which no request widens."
    )
}

/// What the tool that declares a departure from the plan, or an unpassable
/// criterion, does.
pub(crate) fn deviation_description() -> &'static str {
    "Declare, by `kind`, what your task cannot do as the plan says. `departure` declares \
     that your work departs from the plan: what of it (`from`: one of the plan's shapes or \
     decisions by name, one of your task's `changes` by where it is, one of its criteria by \
     its command, or `\"outcome\"`), what the plan says (`planned`), what your work does or \
     needs instead (`instead`), and `why`. Call it whenever you cannot build what the plan \
     declares within your scope: never build something else and say nothing. Your task does \
     not close on a departure; when your session ends a person accepts it or sends it back \
     with what to do instead. Accepted, a criterion of the plan you depart from stops holding \
     your task, and a test the run's spec gave it is written again by whoever wrote the spec. \
     `unpassable` declares that one of your task's criteria (`criterion`, its command) is red \
     on your work and that no work the plan allows can make it pass, with `why`. The engine \
     runs it on your work: one that passes, or that the environment cannot run, is refused; \
     a red one is recorded with its exit, and your attempt ends with it. The suite the run \
     measured holds every task and is no plan's to depart from, under either kind."
}

/// What the tool that asks for a wider scope does.
pub(crate) fn scope_expansion_description() -> &'static str {
    "Ask for the scope you work to be widened — you never widen it yourself. Provide the \
     paths and the reason; for a task, also a verifiable criterion that is red today — or, \
     when one of your task's own criteria fails and its output points at each path as \
     `path:line`, that criterion's command as `evidence`: the engine runs it and, if it \
     still points there, grants the paths itself. A task's request its loop's rules would \
     not grant is tried against every criterion and guard its attempt's check found red, \
     the same way, before anyone is asked. A refusal the project settles comes back in \
     this reply, with its reason. The rest is decided when the attempt ends: a task's by \
     its evidence, its loop's rules or a person, and a denial becomes a finding rather \
     than silence; a node's by a person, who is shown your reason."
}

/// What the tool that reads the group's blackboard does.
pub(crate) fn blackboard_description() -> &'static str {
    "Read this group's blackboard — your OWN posts only while the group runs \
     (siblings' posts become readable after the join, through the \
     group's consolidated output, so results never depend on arrival order)."
}

/// What the tool that submits a document of `kind` does, with the sentence
/// about where its commands run, when it says so.
pub(crate) fn submit_description(kind: ArtifactKind, runs_under: &str) -> String {
    format!(
        "Submit this node's `{kind}` document as a structured object. The engine \
         validates it exactly as the node's close will — unknown keys, wrong types, \
         and the document's own rules — and writes the artifact file itself once it \
         is accepted. A refusal lists every problem to fix; submit again until it is \
         accepted. An acceptance reports what the engine read, so you can see your \
         meaning survived. Submitting again replaces the document.{runs_under}"
    )
}

/// Where a document's commands run, and how the programs they name are found:
/// the shell, and the `PATH` its commands are looked up in.
pub(crate) fn runs_under_sentence(shell: &str, path: &str) -> String {
    format!(
        " Its commands run under `{shell}`, looking programs up in this PATH, in order: {path}. \
         A program found nowhere on it cannot answer, and the document is refused for it."
    )
}
