//! What a box open over a screen covers, held cell by cell against the box drawn over
//! nothing: every cell inside the box, its edges, its words and its blank cells, holds the
//! box's own glyph, tones and ground, and nothing of what stood under it. Drawn on 24 bits,
//! where a pane paints its code and its selection on grounds of their own, so a ground kept
//! from under a box shows. The box's cells are read off the frame it draws over nothing, from
//! its top edge's first round corner to its bottom edge's last.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use surco::components::{Overlay, Region, Screen};
use surco::diagram::Shape;
use surco::doc::Doc;
use surco::look::Look;
use surco::paint::{Ground, Layer, Paint};

/// The paint a box's cells are held in: 24 bits on a dark ground, the ground a terminal that
/// says nothing is taken to draw on.
pub const PAINTED: Paint = Paint::Color {
    layer: Layer::Truecolor(Ground::Dark),
    links: false,
};

/// A screen that holds nothing but `overlay`, open over it: the box drawn over nothing.
pub fn alone(overlay: Overlay) -> Screen {
    let mut alone = Screen::new(Region::new(Doc::new()));
    alone.overlay = Some(overlay);
    alone
}

/// Where `over`, a screen drawn in `look` with a box open over it, keeps something of what
/// stood under the box, against `alone`, the same box drawn over nothing: how many cells
/// inside the box do, and the first, with what it holds and what the box draws there. None
/// where every cell inside the box is the box's own.
pub fn kept_under(over: &Buffer, alone: &Buffer, look: &Look) -> Option<String> {
    let Some(inside) = box_in(alone, look) else {
        return Some("the box drawn over nothing has no round corners".to_string());
    };
    let kept: Vec<Position> = inside
        .positions()
        .filter(|at| over[*at] != alone[*at])
        .collect();
    let first = *kept.first()?;
    let (drawn, own) = (&over[first], &alone[first]);
    Some(format!(
        "{} of the {} cells inside the box keep something of what stood under it; the first, \
         at {},{}, holds `{}` on {:?} in {:?} {:?}, where the box draws `{}` on {:?} in {:?} \
         {:?}",
        kept.len(),
        inside.area(),
        first.x,
        first.y,
        drawn.symbol(),
        drawn.bg,
        drawn.fg,
        drawn.modifier,
        own.symbol(),
        own.bg,
        own.fg,
        own.modifier,
    ))
}

/// The rows over any box, which it never covers: the trail and the rule, whose glyph is a
/// round corner's in ASCII.
const OVER_A_BOX: u16 = 2;

/// The cells of the box `alone` holds over nothing, drawn in `look`: from its top edge's
/// first round corner under the rule to its bottom edge's last, the top edge's last corner
/// found from the right, since a title may hold the glyph an ASCII corner is drawn in; none
/// where no round corner stands.
fn box_in(alone: &Buffer, look: &Look) -> Option<Rect> {
    let [top_left, top_right, bottom_left, _] = look.glyphs.corners(Shape::Round);
    let area = alone.area;
    let holds = |x: u16, y: u16, corner: char| alone[(x, y)].symbol() == corner.to_string();
    let under_the_rule = area.rows().skip(usize::from(OVER_A_BOX));
    let top = under_the_rule
        .flat_map(Rect::positions)
        .find(|at| holds(at.x, at.y, top_left))?;
    let right = (top.x + 1..area.right())
        .rev()
        .find(|x| holds(*x, top.y, top_right))?;
    let bottom = (top.y + 1..area.bottom()).find(|y| holds(top.x, *y, bottom_left))?;
    Some(Rect::new(
        top.x,
        top.y,
        right - top.x + 1,
        bottom - top.y + 1,
    ))
}
