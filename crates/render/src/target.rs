//! The words a tool call is read by: what it did, and what it did it on where
//! the log can show it. The hash is how the log identifies what it cannot show,
//! and only a refused write, which always names a path, still reads a target
//! by itself.

use yunta_core::events::{ToolAction, ToolCall, ToolTarget};

/// The one line a surface shows for a target: what it names when it can
/// be named, and the shortened hash when it cannot.
pub fn line(target: &ToolTarget) -> String {
    match target.display() {
        Some(display) => display.to_string(),
        None => target.digest().abbreviated(),
    }
}

/// What a call acted on, where the log can show it. A plain call has its tool's
/// name to go by, which is the CLI's own and no text of the session's.
fn object<'a>(
    action: ToolAction,
    tool: Option<&'a str>,
    target: Option<&'a ToolTarget>,
) -> Option<&'a str> {
    target.and_then(ToolTarget::display).or(match action {
        ToolAction::Call => tool,
        _ => None,
    })
}

/// One call as a reader is told it: its action, and what it acted on where the
/// log can show it.
pub fn said(action: ToolAction, tool: Option<&str>, target: Option<&ToolTarget>) -> String {
    match object(action, tool, target) {
        Some(object) => format!("{} {object}", action.as_str()),
        None => action.as_str().to_string(),
    }
}

/// One action among a node's calls: how many there were, and the object they
/// all share, when they all share one.
struct Gathered<'a> {
    action: ToolAction,
    object: Option<&'a str>,
    shared: bool,
    count: usize,
}

/// What `calls`, newest first, came to, oldest first: each action once, where
/// its newest call stands in the order, with its object where every call of it
/// shares one, and how many there were where more than one, after `times`.
pub fn gathered(calls: &[ToolCall], times: char) -> Vec<String> {
    let mut groups: Vec<Gathered> = Vec::new();
    for call in calls {
        let object = object(call.action, call.tool_name.as_deref(), call.target.as_ref());
        match groups.iter_mut().find(|group| group.action == call.action) {
            Some(group) => {
                group.count += 1;
                group.shared &= group.object == object;
            }
            None => groups.push(Gathered {
                action: call.action,
                object,
                shared: true,
                count: 1,
            }),
        }
    }
    groups
        .iter()
        .rev()
        .map(|group| {
            let mut said = group.action.as_str().to_string();
            if let (true, Some(object)) = (group.shared, group.object) {
                said.push(' ');
                said.push_str(object);
            }
            if group.count > 1 {
                said.push_str(&format!(" {times}{}", group.count));
            }
            said
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn a_path_the_engine_can_name_reads_as_written() {
        let target = ToolTarget::of_path(Path::new("crates/core/src/lib.rs"));
        assert_eq!(line(&target), "crates/core/src/lib.rs");
    }

    #[test]
    fn a_target_the_session_wrote_reads_as_its_shortened_hash() {
        // The SHA-256 of the empty input, shortened to its first twelve digits.
        assert_eq!(line(&ToolTarget::opaque(b"")), "sha256:e3b0c44298fc");
    }

    fn call(action: ToolAction, target: Option<ToolTarget>) -> ToolCall {
        ToolCall {
            tool_name: Some("Task".to_string()),
            action,
            target,
            at: Default::default(),
        }
    }

    #[test]
    fn a_call_is_told_by_its_action_and_what_the_log_can_show() {
        let path = ToolTarget::of_path(Path::new("src/lib.rs"));
        assert_eq!(
            said(ToolAction::Read, Some("Read"), Some(&path)),
            "read src/lib.rs"
        );
        let secret = ToolTarget::opaque(b"psql postgres://hunter2");
        assert_eq!(said(ToolAction::Run, Some("Bash"), Some(&secret)), "run");
        assert_eq!(
            said(ToolAction::Call, Some("Task"), Some(&secret)),
            "call Task"
        );
        assert_eq!(said(ToolAction::Call, None, None), "call");
    }

    #[test]
    fn calls_gather_by_action_each_where_its_newest_call_stands() {
        let path = |path: &str| Some(ToolTarget::of_path(Path::new(path)));
        let run = || Some(ToolTarget::named("cargo test"));
        // Newest first, as a frame holds them.
        let calls = [
            call(ToolAction::Run, run()),
            call(ToolAction::Read, path("b.rs")),
            call(ToolAction::Edit, path("a.rs")),
            call(ToolAction::Run, run()),
            call(ToolAction::Read, path("a.rs")),
        ];
        assert_eq!(
            gathered(&calls, 'x'),
            ["edit a.rs", "read x2", "run cargo test x2"]
        );
        assert!(gathered(&[], 'x').is_empty());
    }

    #[test]
    fn a_call_with_no_object_keeps_its_fellows_from_sharing_one() {
        let calls = [
            call(ToolAction::Read, None),
            call(
                ToolAction::Read,
                Some(ToolTarget::of_path(Path::new("a.rs"))),
            ),
        ];
        assert_eq!(gathered(&calls, 'x'), ["read x2"]);
    }
}
