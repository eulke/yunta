//! What a tasks document's rules found wrong with it, as the values each
//! clause names. The clause is what a reader acts on; the code is the stable
//! name a receipt counts the rule by.

use serde::{Deserialize, Serialize};

use super::suite::outside_clause;
use crate::diagnostic::RuleCode;
use crate::{QuestionId, ScopeGlob, TaskId};

/// What a rule of a tasks document found wrong with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TaskFault {
    NoSummary,
    NoPlanDescription,
    UnexplainedDecision {
        decision: String,
        choice: String,
    },
    ChangeWithoutCode {
        at: String,
    },
    NoTaskDescription,
    NoOutcome,
    NoChanges,
    UnexplainedCriterion {
        cmd: String,
    },
    DuplicateTask,
    UnknownDependency {
        dependency: TaskId,
    },
    EmptyTitle,
    EmptyScope,
    NoCriteria,
    AllCriteriaAreGuards,
    DependencyCycle {
        cycle: Vec<TaskId>,
    },
    OverlappingScope {
        glob: ScopeGlob,
        other: TaskId,
        other_glob: ScopeGlob,
    },
    ChangesASpecTest {
        at: String,
        owner: TaskId,
    },
    UnknownAnswer {
        decision: String,
        question: QuestionId,
        answered: Vec<QuestionId>,
    },
    CriterionChecksPresence {
        cmd: String,
        file: String,
    },
    OutsideTheSuite {
        command: String,
        suite: String,
    },
    UsesItsOwnShape {
        shape: String,
    },
    TaskWritesItsTest {
        at: String,
        cmd: String,
    },
    /// `cmd` is the criterion's command with its surrounding space trimmed,
    /// as the clause names it.
    SharedCriterion {
        cmd: String,
        owner: TaskId,
    },
    DuplicateShape {
        shape: String,
    },
    DuplicateDecision {
        decision: String,
    },
    UnknownShapeOwner {
        shape: String,
        owner: TaskId,
    },
    /// `unreadable` is the report of a scope that does not compile, when that
    /// is why the file cannot be shown to be covered.
    ShapeOutsideOwnerScope {
        shape: String,
        file: String,
        owner: TaskId,
        unreadable: Option<String>,
    },
    UnknownShape {
        shape: String,
    },
    ShapeUsedBeforeItsOwner {
        shape: String,
        owner: TaskId,
    },
    /// `unreadable` is the report of a scope that does not compile, when that
    /// is why the change cannot be judged against it.
    ChangeOutsideScope {
        file: String,
        unreadable: Option<String>,
    },
    /// `names` are the bare identifiers; the clause puts the backticks around them.
    ChangeCodeMissesAName {
        at: String,
        names: Vec<String>,
    },
    ChangeCodeIsAComment {
        at: String,
    },
}

impl TaskFault {
    /// The stable name of the rule this fault breaks.
    pub fn code(&self) -> RuleCode {
        match self {
            Self::NoSummary => RuleCode::NoSummary,
            Self::NoPlanDescription | Self::NoTaskDescription => RuleCode::NoDescription,
            Self::UnexplainedDecision { .. } => RuleCode::UnexplainedDecision,
            Self::ChangeWithoutCode { .. } => RuleCode::ChangeWithoutCode,
            Self::NoOutcome => RuleCode::NoOutcome,
            Self::NoChanges => RuleCode::NoChanges,
            Self::UnexplainedCriterion { .. } => RuleCode::UnexplainedCriterion,
            Self::DuplicateTask => RuleCode::DuplicateId,
            Self::UnknownDependency { .. } => RuleCode::UnknownDependency,
            Self::EmptyTitle => RuleCode::EmptyTitle,
            Self::EmptyScope => RuleCode::EmptyScope,
            Self::NoCriteria => RuleCode::NoCriteria,
            Self::AllCriteriaAreGuards => RuleCode::AllCriteriaAreGuards,
            Self::DependencyCycle { .. } => RuleCode::DependencyCycle,
            Self::OverlappingScope { .. } => RuleCode::OverlappingScope,
            Self::ChangesASpecTest { .. } => RuleCode::ChangesASpecTest,
            Self::UnknownAnswer { .. } => RuleCode::UnknownAnswer,
            Self::CriterionChecksPresence { .. } => RuleCode::CriterionChecksPresence,
            Self::OutsideTheSuite { .. } => RuleCode::OutsideTheSuite,
            Self::UsesItsOwnShape { .. } => RuleCode::UsesItsOwnShape,
            Self::TaskWritesItsTest { .. } => RuleCode::TaskWritesItsTest,
            Self::SharedCriterion { .. } => RuleCode::SharedCriterion,
            Self::DuplicateShape { .. } => RuleCode::DuplicateShape,
            Self::DuplicateDecision { .. } => RuleCode::DuplicateDecision,
            Self::UnknownShapeOwner { .. } => RuleCode::UnknownShapeOwner,
            Self::ShapeOutsideOwnerScope { .. } => RuleCode::ShapeOutsideOwnerScope,
            Self::UnknownShape { .. } => RuleCode::UnknownShape,
            Self::ShapeUsedBeforeItsOwner { .. } => RuleCode::ShapeUsedBeforeItsOwner,
            Self::ChangeOutsideScope { .. } => RuleCode::ChangeOutsideScope,
            Self::ChangeCodeMissesAName { .. } => RuleCode::ChangeCodeMissesAName,
            Self::ChangeCodeIsAComment { .. } => RuleCode::ChangeCodeIsAComment,
        }
    }

    /// The clause a reader acts on, in the words the rule has always been
    /// written in.
    pub fn clause(&self) -> String {
        match self {
            Self::NoSummary => NO_SUMMARY.to_string(),
            Self::NoPlanDescription => NO_PLAN_DESCRIPTION.to_string(),
            Self::UnexplainedDecision { decision, choice } => unexplained(decision, choice),
            Self::ChangeWithoutCode { at } => change_without_code(at),
            Self::NoTaskDescription => NO_TASK_DESCRIPTION.to_string(),
            Self::NoOutcome => NO_OUTCOME.to_string(),
            Self::NoChanges => NO_CHANGES.to_string(),
            Self::UnexplainedCriterion { cmd } => unexplained_criterion(cmd),
            Self::DuplicateTask => DUPLICATE_TASK.to_string(),
            Self::UnknownDependency { dependency } => unknown_dependency(dependency),
            Self::EmptyTitle => EMPTY_TITLE.to_string(),
            Self::EmptyScope => EMPTY_SCOPE.to_string(),
            Self::NoCriteria => NO_CRITERIA.to_string(),
            Self::AllCriteriaAreGuards => ALL_CRITERIA_ARE_GUARDS.to_string(),
            Self::DependencyCycle { cycle } => dependency_cycle(cycle),
            Self::OverlappingScope {
                glob,
                other,
                other_glob,
            } => overlapping_scope(glob, other, other_glob),
            Self::ChangesASpecTest { at, owner } => changes_a_spec_test(at, owner),
            Self::UnknownAnswer {
                decision,
                question,
                answered,
            } => unknown_answer(decision, question, answered),
            Self::CriterionChecksPresence { cmd, file } => criterion_checks_presence(cmd, file),
            Self::OutsideTheSuite { command, suite } => outside_clause(command, suite),
            Self::UsesItsOwnShape { shape } => uses_its_own_shape(shape),
            Self::TaskWritesItsTest { at, cmd } => task_writes_its_test(at, cmd),
            Self::SharedCriterion { cmd, owner } => shared_criterion(cmd, owner),
            Self::DuplicateShape { shape } => duplicate_shape(shape),
            Self::DuplicateDecision { decision } => duplicate_decision(decision),
            Self::UnknownShapeOwner { shape, owner } => unknown_shape_owner(shape, owner),
            Self::ShapeOutsideOwnerScope {
                shape,
                file,
                owner,
                unreadable,
            } => shape_outside_owner_scope(shape, file, owner, unreadable.as_deref()),
            Self::UnknownShape { shape } => unknown_shape(shape),
            Self::ShapeUsedBeforeItsOwner { shape, owner } => used_before_owner(shape, owner),
            Self::ChangeOutsideScope { file, unreadable } => outside_scope(file, unreadable),
            Self::ChangeCodeMissesAName { at, names } => change_code_misses_a_name(at, names),
            Self::ChangeCodeIsAComment { at } => change_code_is_a_comment(at),
        }
    }
}

// The clauses that name no value, each whole as the writer reads it.
const NO_SUMMARY: &str = "`summary` is missing; say what the plan changes, in one line";
const NO_PLAN_DESCRIPTION: &str =
    "`description` is missing; say what changes, why and how the work is approached";
const NO_TASK_DESCRIPTION: &str = "`description` is missing; say what the task does and why";
const NO_OUTCOME: &str =
    "`outcome` is missing; say what a person will observe once the task is done";
const NO_CHANGES: &str = "`changes` is empty; say what the task changes, place by place";
const DUPLICATE_TASK: &str = "a second task already carries this id; every id is declared once";
const EMPTY_TITLE: &str = "`title` is empty";
const EMPTY_SCOPE: &str =
    "`scope` is empty; every task declares at least one glob, the only paths it may touch";
const NO_CRITERIA: &str =
    "no criteria declared; every task needs at least one command that verifies it";
const ALL_CRITERIA_ARE_GUARDS: &str = "every criterion is a `guard`; at least one must be able to \
     fail before the work, or there is nothing the work has to make pass";

fn unexplained(decision: &str, choice: &str) -> String {
    format!("decision `{decision}` does not say `why` it chose `{choice}`")
}

fn change_without_code(at: &str) -> String {
    format!(
        "the change at `{at}` shows no `code`; give the declarations it adds or changes — each \
         type with its fields, each signature whole — so a person sees how the work will look"
    )
}

fn unexplained_criterion(cmd: &str) -> String {
    format!("`{cmd}` does not say what it `proves`")
}

fn unknown_dependency(dependency: &TaskId) -> String {
    format!("`depends_on` names `{dependency}`, which no task in this file declares")
}

fn dependency_cycle(cycle: &[TaskId]) -> String {
    let path: Vec<&str> = cycle.iter().map(TaskId::as_str).collect();
    format!("`depends_on` forms a cycle: {}", path.join(" -> "))
}

fn overlapping_scope(glob: &ScopeGlob, other: &TaskId, other_glob: &ScopeGlob) -> String {
    format!(
        "`{glob}` overlaps task `{other}`'s `{other_glob}`, and neither depends on the other; \
         give them disjoint scopes or declare the dependency"
    )
}

fn changes_a_spec_test(at: &str, owner: &TaskId) -> String {
    format!(
        "`changes` names `{at}`, a test the spec wrote for `{owner}`; no session may write it — \
         drop the change, or adjust the plan so the test asks for what it needs"
    )
}

fn unknown_answer(decision: &str, question: &QuestionId, answered: &[QuestionId]) -> String {
    format!(
        "decision `{decision}` answers `{question}`, which is no question a person answered in \
         this run{}",
        answered_list(answered)
    )
}

/// What a clause adds naming the questions a person answered: nothing when
/// none was answered at all.
fn answered_list(answered: &[QuestionId]) -> String {
    match answered {
        [] => String::new(),
        _ => format!(
            "; it answered {}",
            crate::text::listed(answered.iter().map(QuestionId::as_str))
        ),
    }
}

fn criterion_checks_presence(cmd: &str, file: &str) -> String {
    format!(
        "`{cmd}` passes once a name is written in `{file}`, whatever the code does; run the test \
         that observes the behavior"
    )
}

fn uses_its_own_shape(shape: &str) -> String {
    format!(
        "`uses` names `{shape}`, which this task builds; a task's own shapes are in its \
         `changes`, and `uses` names those another task builds"
    )
}

fn task_writes_its_test(at: &str, cmd: &str) -> String {
    format!(
        "`changes` names `{at}`, the test its criterion `{cmd}` runs; the spec writes a task's \
         tests — keep the file in `scope`, out of `changes`, and name a test the spec will write"
    )
}

fn shared_criterion(cmd: &str, owner: &TaskId) -> String {
    format!(
        "`{cmd}` also judges `{owner}`: once `{owner}` lands it passes, and this task is blocked \
         before any work — give it a test of its own"
    )
}

fn duplicate_shape(shape: &str) -> String {
    format!("shape `{shape}` is declared twice; every shape is declared once")
}

fn duplicate_decision(decision: &str) -> String {
    format!("decision `{decision}` is declared twice; every decision has an id of its own")
}

fn unknown_shape_owner(shape: &str, owner: &TaskId) -> String {
    format!("shape `{shape}` is owned by `{owner}`, which no task in this file declares")
}

fn shape_outside_owner_scope(
    shape: &str,
    file: &str,
    owner: &TaskId,
    unreadable: Option<&str>,
) -> String {
    match unreadable {
        None => format!(
            "shape `{shape}` lives in `{file}`, which its owner `{owner}`'s scope does not cover; \
             the task that builds a shape may write its file"
        ),
        Some(report) => format!(
            "shape `{shape}` lives in `{file}`, but its owner `{owner}`'s scope does not compile \
             into one set of paths, so it cannot be shown to cover the file: {report}"
        ),
    }
}

fn unknown_shape(shape: &str) -> String {
    format!("`uses` names shape `{shape}`, which `shapes` does not declare")
}

fn used_before_owner(shape: &str, owner: &TaskId) -> String {
    format!(
        "uses shape `{shape}`, which `{owner}` builds, without waiting for it; add `{owner}` to \
         `depends_on`"
    )
}

fn outside_scope(file: &str, unreadable: &Option<String>) -> String {
    match unreadable.as_deref() {
        None => format!(
            "changes `{file}`, which its scope does not cover; widen the scope, or leave the \
             change to the task that owns the file"
        ),
        Some(report) => format!(
            "changes `{file}`, but its scope does not compile into one set of paths, so the \
             change cannot be judged against it: {report}"
        ),
    }
}

fn change_code_misses_a_name(at: &str, names: &[String]) -> String {
    let named: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    format!(
        "the change at `{at}` names {}, and its `code` does not show {}; give the declaration of \
         each — its whole signature, or the type with the fields it adds",
        named.join(", "),
        if names.len() == 1 { "it" } else { "them" }
    )
}

fn change_code_is_a_comment(at: &str) -> String {
    format!(
        "the change at `{at}` has only a comment for its `code`; give the code it adds or changes \
         — a comment says what the code will do, and a person decides on how it looks"
    )
}

#[cfg(test)]
mod tests {
    use super::TaskFault;
    use crate::{QuestionId, ScopeGlob, TaskId};

    #[test]
    fn each_clause_reads_as_the_plan_has_always_said_it() {
        let shape = |unreadable: Option<String>| TaskFault::ShapeOutsideOwnerScope {
            shape: "PackStore".to_string(),
            file: "src/pack.rs".to_string(),
            owner: TaskId::from("t1"),
            unreadable,
        };
        assert_eq!(
            shape(None).clause(),
            "shape `PackStore` lives in `src/pack.rs`, which its owner `t1`'s scope does not cover; \
             the task that builds a shape may write its file"
        );
        assert_eq!(
            shape(Some("unclosed [".to_string())).clause(),
            "shape `PackStore` lives in `src/pack.rs`, but its owner `t1`'s scope does not compile \
             into one set of paths, so it cannot be shown to cover the file: unclosed ["
        );
        let change = |unreadable: Option<String>| TaskFault::ChangeOutsideScope {
            file: "src/a.rs".to_string(),
            unreadable,
        };
        assert_eq!(
            change(None).clause(),
            "changes `src/a.rs`, which its scope does not cover; widen the scope, or leave the \
             change to the task that owns the file"
        );
        assert_eq!(
            change(Some("unclosed [".to_string())).clause(),
            "changes `src/a.rs`, but its scope does not compile into one set of paths, so the \
             change cannot be judged against it: unclosed ["
        );
    }

    #[test]
    fn plain_and_answer_clauses_read_as_written() {
        assert_eq!(TaskFault::EmptyTitle.clause(), "`title` is empty");
        assert_eq!(
            TaskFault::UnknownAnswer {
                decision: "d1".to_string(),
                question: QuestionId::from("q1"),
                answered: Vec::new(),
            }
            .clause(),
            "decision `d1` answers `q1`, which is no question a person answered in this run"
        );
        assert_eq!(
            TaskFault::UnknownAnswer {
                decision: "d1".to_string(),
                question: QuestionId::from("q1"),
                answered: vec![QuestionId::from("q2"), QuestionId::from("q3")],
            }
            .clause(),
            "decision `d1` answers `q1`, which is no question a person answered in this run; it \
             answered `q2`, `q3`"
        );
    }

    #[test]
    fn listed_values_read_as_written() {
        assert_eq!(
            TaskFault::ChangeCodeMissesAName {
                at: "src/a.rs::add/update".to_string(),
                names: vec!["add".to_string(), "update".to_string()],
            }
            .clause(),
            "the change at `src/a.rs::add/update` names `add`, `update`, and its `code` does not \
             show them; give the declaration of each — its whole signature, or the type with the \
             fields it adds"
        );
        assert_eq!(
            TaskFault::OverlappingScope {
                glob: ScopeGlob::from("src/**"),
                other: TaskId::from("t2"),
                other_glob: ScopeGlob::from("src/a/**"),
            }
            .clause(),
            "`src/**` overlaps task `t2`'s `src/a/**`, and neither depends on the other; give them \
             disjoint scopes or declare the dependency"
        );
        assert_eq!(
            TaskFault::OutsideTheSuite {
                command: "cargo test -p x".to_string(),
                suite: "cargo test --workspace".to_string(),
            }
            .clause(),
            "`cargo test -p x` runs the project's tests outside the suite `cargo test --workspace`, \
             so it builds them again beside the suite's build; extend the suite instead — `cargo \
             test --workspace <selector>`, where `<selector>` picks this task's tests"
        );
    }
}
