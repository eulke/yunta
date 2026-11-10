//! The words a task moment is said in: a task registered, moved, checked
//! and answered, a departure from the plan and its answer, a criterion no
//! work can pass, and what the tree held of the shapes read for a plan.

use surco::figures::duration;
use surco::text::{counted, detailed};
use yunta_core::events::tasks::happening::Happening;

pub(super) fn task_words(happening: &Happening) -> String {
    use Happening as H;
    match happening {
        H::Registered { task } => format!("task {task} registered"),
        H::Moved { task, to } => format!("{task} is {}", to.as_str()),
        H::Checking { task } => format!("{task} asked for a check"),
        H::Checked {
            task,
            closes,
            red,
            duration_ms,
        } => {
            let took = duration(std::time::Duration::from_millis(*duration_ms));
            match (closes, red) {
                (true, _) => format!("{task} checked in {took}: it would close"),
                (false, 0) => {
                    format!("{task} checked in {took}: its work reaches paths it may not change")
                }
                (false, red) => format!("{task} checked in {took}: {red} of its criteria red"),
            }
        }
        H::Departed { task, from } => format!("{task} departs from the plan: {from}"),
        H::Answered { task, accepted } => match accepted {
            true => format!("{task}'s departure from the plan was accepted"),
            false => format!("{task}'s departure from the plan was sent back"),
        },
        H::Unpassable {
            task,
            criterion,
            why,
        } => detailed(format!("{task} declared `{criterion}` cannot pass"), why),
        H::ShapesRead {
            shapes,
            held,
            outside,
        } => shapes_read(*shapes, *held, *outside),
    }
}

/// What the tree a plan was registered against holds of the `shapes` read
/// for it: `held` of them, named in `outside` files no task changes. They
/// can be fewer than the plan declares, since a file format or a schema is
/// no word of code to read.
fn shapes_read(shapes: usize, held: usize, outside: usize) -> String {
    let holds = match (shapes, held) {
        (1, 1) => "the tree holds the one shape read".to_string(),
        (1, _) => "the tree does not hold the one shape read".to_string(),
        (_, 0) => format!("the tree holds none of the {shapes} shapes read"),
        _ => format!("the tree holds {held} of the {shapes} shapes read"),
    };
    match outside {
        0 => holds,
        _ => format!(
            "{holds}; named in {} no task changes",
            counted(outside, "file")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reading is said by how many shapes it read, not by how many the
    /// plan declares.
    #[test]
    fn a_reading_says_what_the_tree_holds_of_the_shapes_it_read() {
        let said = |shapes, held, outside| {
            task_words(&Happening::ShapesRead {
                shapes,
                held,
                outside,
            })
        };
        assert_eq!(
            said(1, 1, 1),
            "the tree holds the one shape read; named in 1 file no task changes"
        );
        assert_eq!(said(1, 0, 0), "the tree does not hold the one shape read");
        assert_eq!(said(3, 0, 0), "the tree holds none of the 3 shapes read");
        assert_eq!(
            said(4, 2, 5),
            "the tree holds 2 of the 4 shapes read; named in 5 files no task changes"
        );
    }
}
