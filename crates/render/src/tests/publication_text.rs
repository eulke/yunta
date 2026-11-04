//! A text document on a pull request: it has no code lines or folds to give up, so its
//! words go before every document does.

use std::path::PathBuf;

use yunta_core::events::{ArtifactId, Shown};
use yunta_core::shown::{ShownContent, ShownDocument};
use yunta_core::ContentHash;

use super::documents::global_packs;
use super::publication::{assert_within, described, least_limit, publishing};

/// A brief a gate shows, opening on a heading, far longer than a small limit.
fn long_brief() -> ShownDocument {
    let text = format!("# What the brief asks\n\n{}", "words ".repeat(2_000));
    ShownDocument {
        shown: Shown {
            producer: Some("brief".into()),
            artifact: ArtifactId::Opaque {
                name: "brief.md".to_string(),
            },
            content_hash: ContentHash::sha256(text.as_bytes()),
        },
        path: PathBuf::from("artifacts/brief/brief.md"),
        content: ShownContent::Text(text),
    }
}

/// Just over what the page takes with the brief down to its title, the title stays, what
/// it says goes, and the note says so and where it is whole.
#[test]
fn a_description_too_long_for_a_text_document_keeps_its_title() {
    let mut request = publishing(&global_packs());
    request.shown = vec![long_brief()];
    let limit = least_limit(&request, |page| {
        page.contains("## What the brief asks: brief.md")
    });
    let page = described(&request, limit);
    assert_within(&page, limit);
    assert!(!page.contains("words words"), "{page}");
    assert!(page.contains("what each text document says"), "{page}");
    assert!(page.contains("hold every document whole"), "{page}");
}
