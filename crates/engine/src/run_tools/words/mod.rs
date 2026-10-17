//! The words the engine says to an agent through its run tools: what a
//! session is told ahead of any call it makes, how each tool is described, what
//! a submission and a handover answer, what a refused call reads back, what a
//! stop says, and how a request the project settled reads.
//!
//! The engine names no command here. A sentence says what happened in the
//! domain's own terms, and a person's words are written where a person
//! reads them, so this module is the one place an agent's sentences live.
//! Each file holds the sentences of one surface. The code that answers a call
//! decides which sentence applies, and passes the values it names.

mod catalog;
mod handover;
mod notice;
mod scope;
mod session;
mod stop;
mod submission;
mod verdicts;

pub(crate) use catalog::{
    answer_finding_description, answer_id_description, answer_node_description,
    artifact_name_description, blackboard_description, check_artifact_description,
    check_scope_description, check_task_description, deviation_description, findings_description,
    post_finding_description, runs_under_sentence, scope_expansion_description, submit_description,
    task_description, task_status_description, update_finding_description,
    withdraw_finding_description,
};
pub(crate) use handover::could_not_write;
pub(crate) use notice::{
    approved_tests_text, departure_text, documents_text, files_text, findings_text, naming_text,
    plan_text, respecify_continued_text, respecify_fresh_text, review_continued_text,
    review_fresh_text, scope_answer_text, scope_read_node_text, scope_read_task_text, task_text,
};
pub(crate) use scope::request_next;
pub(crate) use session::{known_list, task_brief, tool_error, unknown_tool};
pub(crate) use stop::stop_unfinished;
pub(crate) use submission::{
    accepted, artifact_verdict, document_missing, failing_now, report_each_finding,
    submission_refused,
};
pub(crate) use verdicts::{
    already_declared, amended, declared, denied, departure_recorded, finding_refusal_heading,
    recorded, reply_text,
};
