//! A region that scrolls: what of it a person asked to have in sight, and what a surface
//! laid out of it. Where a person scrolled to is kept by whoever answers their keys, so
//! every interface scrolls by the same keys and comes back to the same line; how many
//! lines fit is the surface's, which hands back what it laid out for the next key to move
//! from rather than keep it in the drawing. A surface that shows a whole document at once,
//! as `Markdown` and `Html` do, reads neither.

/// What of a region a person asked to have in sight. Not [`Laid`]: what a surface showed
/// of it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sight {
    /// The first of its lines, as a surface lays them out, a person asked to see. Past
    /// the last first line a scroll can reach, its last lines show.
    pub top: usize,
    /// Whether the line a pointer stands on has to be in sight, whatever `top` says.
    pub follows_pointer: bool,
}

impl Sight {
    /// From the line `top` on.
    pub fn from_line(top: usize) -> Self {
        Sight {
            top,
            follows_pointer: false,
        }
    }

    /// From its first line, moved as far as the line a pointer stands on is in sight: a
    /// list a person moves along.
    pub fn following_pointer() -> Self {
        Sight {
            top: 0,
            follows_pointer: true,
        }
    }

    /// Its last lines: what grows at its end, as a run's chronicle does, read where it
    /// grows.
    pub fn at_the_end() -> Self {
        Sight::from_line(usize::MAX)
    }

    /// Its last lines, moved as far as the line a pointer stands on is in sight: a
    /// question asked under what it is about, and the choice pointed at.
    pub fn at_the_end_with_the_pointer() -> Self {
        Sight {
            top: usize::MAX,
            follows_pointer: true,
        }
    }
}

/// What a surface laid out of a region that scrolls: the first of its lines it shows, how
/// many lines a page of it holds, and the last first line a scroll can reach. Not
/// [`Sight`]: what a person asked to see of it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Laid {
    pub top: usize,
    pub page: usize,
    pub most: usize,
}

impl Laid {
    /// Whether more of the region stands than a page of it shows.
    pub fn scrolls(self) -> bool {
        self.most > 0
    }
}
