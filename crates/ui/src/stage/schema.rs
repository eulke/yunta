//! See [`super`]. The shapes of the documents Yunta reads, opened from the workflows
//! with `s`: every document, each with what it is for, a person narrows by typing and
//! picks with Enter; then its shape, as `yunta schema <name>` prints it, which scrolls
//! where it is taller than what shows of it. Escape goes back to the documents, and
//! from them to where the shapes were opened. Nothing is asked of Yunta: the shapes are
//! core's.

use surco::components::{Laid, Prose, Region, Screen, Sight};
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::DocumentKind;
use yunta_render::page::{Page, Schema};
use yunta_render::prompt::Escape;
use yunta_render::screen;

use super::Want;
use crate::prompt::{Alternative, Answerable, Picker, Press, Stroke};
use crate::scroll::Scroll;

/// The documents Yunta reads, and the shape of the one picked.
#[derive(Debug, Clone)]
pub struct SchemaStage {
    /// The documents, as far as what was typed narrows them.
    documents: Picker<DocumentKind>,
    /// The document whose shape stands, once one is picked.
    picked: Option<DocumentKind>,
    /// Where its shape is scrolled to.
    scroll: Scroll,
}

impl SchemaStage {
    /// Every document Yunta reads, in the order `yunta schema` lists them, the pointer on
    /// the first.
    pub(crate) fn new() -> Self {
        let documents = DocumentKind::all()
            .map(|kind| Alternative {
                name: kind.as_str().to_string(),
                label: Some(kind.purpose().to_string()),
                value: kind,
            })
            .collect();
        SchemaStage {
            documents: Picker::new(documents, Escape::GoesBack),
            picked: None,
            scroll: Scroll::default(),
        }
    }

    /// The documents to pick from, or the shape of the one picked; with the keys.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let mut trail = vec![screen::YUNTA.to_string(), screen::SCHEMA.to_string()];
        let mut shown = match self.picked {
            Some(kind) => {
                trail.push(kind.as_str().to_string());
                let region = Region::new(Schema::of(kind).document(look));
                let mut shown = Screen::new(region.in_sight(self.scroll.sight()));
                let legend = screen::shape_legend();
                shown.legend = match self.scroll.scrolls() {
                    true => screen::scrolling(legend),
                    false => legend,
                };
                shown
            }
            None => {
                let doc = Doc::new()
                    .with(Prose(screen::WHICH_DOCUMENT.to_string()))
                    .with(self.documents.block(look));
                Screen::new(Region::new(doc).in_sight(Sight::following_pointer()))
            }
        };
        shown.trail = trail;
        shown
    }

    /// What `stroke` asks for here. Among the documents, typing narrows them, Enter picks
    /// the one pointed at and Escape goes back. On a shape, Escape goes back to the
    /// documents, `q` leaves, and the keys that scroll scroll it. Ctrl-C leaves.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        if self.picked.is_some() {
            return match stroke {
                Stroke::Decline => {
                    self.picked = None;
                    Vec::new()
                }
                Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
                other => {
                    self.scroll.pressed(other);
                    Vec::new()
                }
            };
        }
        match self.documents.press(stroke) {
            Press::Answer(kind) => {
                self.picked = Some(kind);
                self.scroll.rewound();
                Vec::new()
            }
            Press::Decline => vec![Want::Back],
            Press::Interrupt => vec![Want::Leave],
            Press::Stay | Press::Moved => Vec::new(),
        }
    }

    /// What a surface laid out of a shape, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        if self.picked.is_some() {
            self.scroll.laid(laid);
        }
    }
}
