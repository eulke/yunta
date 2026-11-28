//! See [`super`]. The shapes of the documents Yunta reads, opened from the workflows
//! with `s`: every document, each with what it is for, under the question, a person
//! narrows by typing, moves along with the arrows and picks with Enter; then its shape, as
//! `yunta schema <name>` prints it, a page which scrolls where it is taller than what shows
//! of it. Escape goes back to the documents, and from them to where the shapes were opened;
//! `n` opens a new run over a shape, and `q` goes down to the field. It stands under the workflows on the trail, in the
//! frame of the field it was opened over (`over_the_field`), where `a` goes where its
//! attention row names over a shape and types among the documents. Nothing is asked of
//! Yunta: the shapes are core's.

use chrono::{DateTime, Utc};
use surco::components::{ChoiceList, Laid, Legend, Region, Screen, Sight};
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::step::{Arg, NextStep};
use yunta_core::DocumentKind;
use yunta_render::page::{Page, Schema};
use yunta_render::prompt::Escape;
use yunta_render::screen;

use super::over_the_field::{attending_over_the_field, in_the_fields_frame, KeyA};
use super::Want;
use crate::prompt::{Alternative, Answerable, Picker, Press, Stroke};
use crate::scroll::Scroll;
use crate::FollowedRuns;

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
                label: Some(kind.purpose().to_string()),
                ..Alternative::named(kind.as_str(), kind)
            })
            .collect();
        SchemaStage {
            documents: Picker::new(documents, Escape::GoesBack),
            picked: None,
            scroll: Scroll::default(),
        }
    }

    /// The documents to pick from under the question, or the shape of the one picked as a
    /// page, in the frame of the field `followed` keeps at `now`, a line typed in what stands
    /// over it where `typed`; with the keys, those that move along the documents in the
    /// legend's row.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        let mut trail = vec![
            screen::YUNTA.to_string(),
            screen::WORKFLOWS.to_string(),
            screen::SCHEMA.to_string(),
        ];
        let mut shown = match self.picked {
            Some(kind) => {
                trail.push(kind.as_str().to_string());
                let region = Region::of_a_page(Schema::of(kind).document(look));
                let mut shown = Screen::new(region.in_sight(self.scroll.sight()));
                let new_run = followed.can_ask_for_a_run();
                shown.legend = screen::page_legend(screen::PageRead::Whole, Legend::new(), new_run);
                shown
            }
            None => {
                let documents = ChoiceList {
                    choices: self.documents.list(),
                    filter: self.documents.filter().map(str::to_string),
                    legend: Legend::new(),
                };
                let doc = Doc::new()
                    .with(vec![screen::which_document_asks()])
                    .with(documents);
                let mut shown = Screen::new(Region::new(doc).in_sight(Sight::following_pointer()));
                shown.legend = screen::documents_legend();
                shown.typed = true;
                shown
            }
        };
        shown.trail = trail;
        let typed = typed || shown.typed;
        in_the_fields_frame(&mut shown, followed, now, typed, KeyA::Attends);
        shown
    }

    /// Whether what is typed narrows the documents: while none is picked.
    pub(crate) fn typing(&self) -> bool {
        self.picked.is_none()
    }

    /// What `stroke` asks for here, over what `followed` keeps at `now`. Among the
    /// documents, typing narrows them, Enter picks the one pointed at and Escape goes back.
    /// On a shape, Escape goes back to the documents, `a` goes where the attention row names,
    /// `n` opens a new run, `q` goes down to the field, and the keys that scroll scroll it.
    /// Ctrl-C leaves.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        if self.picked.is_some() {
            return match stroke {
                Stroke::Decline => {
                    self.picked = None;
                    Vec::new()
                }
                Stroke::Insert('a') => attending_over_the_field(followed, now),
                Stroke::Insert('n') if followed.can_ask_for_a_run() => {
                    Want::taking(&NextStep::Run {
                        workflow: Arg::Blank,
                    })
                }
                Stroke::Insert('q') => vec![Want::ToTheField(None)],
                Stroke::Interrupt => vec![Want::Leave],
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

    /// Where a shape picked stands as a page, which no row of its screen says, its legend the
    /// same whether it scrolls or not; none while the documents are picked from.
    pub(crate) fn page(&self) -> Option<Scroll> {
        self.picked.map(|_| self.scroll)
    }

    /// What a surface laid out of a shape, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        if self.picked.is_some() {
            self.scroll.laid(laid);
        }
    }
}
