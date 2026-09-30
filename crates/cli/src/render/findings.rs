//! What a review found, as the person deciding reads it on a terminal:
//! how many of each severity, then each finding, the most severe first —
//! what is wrong, where, and in the reviewer's own words what goes wrong
//! and when.

use yunta_core::{FindingEntry, FindingsFile};

use crate::render::markdown::{hanging, markdown};
use crate::render::INDENT;

/// Where a finding's facts sit: one step under its headline.
const BODY: &str = "    ";

pub(super) fn findings(file: &FindingsFile, of: &str, width: usize) -> Vec<String> {
    let mut found: Vec<&FindingEntry> = file.findings.iter().collect();
    found.sort_by_key(|finding| finding.severity);
    let counts: Vec<String> = found
        .chunk_by(|a, b| a.severity == b.severity)
        .filter_map(|same| {
            let severity = same.first()?.severity;
            Some(format!("{} {}", same.len(), severity.as_str()))
        })
        .collect();
    let mut lines = vec![match counts.is_empty() {
        true => format!("the findings{of} — none"),
        false => format!("the findings{of} — {}", counts.join(", ")),
    }];
    for finding in found {
        lines.push(String::new());
        lines.extend(hanging(
            INDENT,
            &format!("{} — ", finding.severity.as_str()),
            &finding.title,
            width,
        ));
        lines.extend(hanging(
            BODY,
            "",
            &format!("{}, at {}", finding.id, finding.location),
            width,
        ));
        lines.extend(markdown(&finding.detail, BODY, width));
    }
    lines
}
