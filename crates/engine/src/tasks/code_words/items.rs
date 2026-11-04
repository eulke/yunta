//! What a file holds of the items a shape declares, read the way the shape
//! writes them: a function to its signature where the shape writes it with
//! no body, a block of members (an `impl`, a `trait`, a module, a class) to
//! the members the shape names, and attributes or decorators only where the
//! shape writes some.
//!
//! Both texts are split into items by surco's item reader, in Rust, Go,
//! JavaScript or TypeScript, and Python; a file's are searched at its top,
//! one level into each `impl`, `trait`, class and interface, and one level
//! into each inline module that is not a test's.
//!
//! The reader can pick the wrong item where a macro makes items, where an
//! inline module other than a test's holds a namesake, or where `cfg` twins
//! declare one and the plan writes neither twin's attributes: it then reads the
//! twins in turn, the first for the first planned. Where it cannot tell, a
//! shape reads as not compared or as new.

use std::collections::BTreeSet;
use std::ops::Range;

use surco::code::items::{Item, Kind, Text};
use surco::code::Language;

/// What `source`, a file in `language`, holds of the items `planned`
/// declares, each read as `planned` writes it, in its order, joined by a
/// blank line; `None` when it holds none of them, or when the items of its
/// language are not read.
pub(crate) fn as_planned(source: &str, planned: &str, language: Language) -> Option<String> {
    let today = Text::of(source, language)?;
    let plan = Text::of(planned, language)?;
    let held = Held::of(&today);
    // Where each of today's items an earlier planned item read starts.
    let mut read = BTreeSet::new();
    let mut readings = Vec::new();
    for wanted in plan.items() {
        if let Some((among, found)) = held.found(&today, (&plan, &wanted), &read) {
            let reading = reading((&today, among, found), (&plan, &wanted), "", &mut read);
            readings.push(reading);
        }
    }
    (!readings.is_empty()).then(|| readings.join("\n\n"))
}

/// Today's `item`, one of the items `among`, read the way the plan writes
/// `planned`, its first line set in by `indent`: a function to its
/// signature where the plan writes it with no body, ended as the plan ends
/// it; a block of members to the members the plan names, each set in one
/// level further; and any other item whole. Its attributes are read only
/// where the plan writes some. What it reads joins `read`. An item a group
/// declares, where the plan writes it alone, reads with the group's keyword.
fn reading(
    (today, among, item): (&Text, &[Item], &Item),
    (plan, planned): (&Text, &Item),
    indent: &str,
    read: &mut BTreeSet<usize>,
) -> String {
    // A group writes its keyword once for every item it declares; an item
    // the plan writes alone reads with it, as the plan writes it.
    let lead = match (&item.lead, &planned.lead) {
        (Some(lead), None) => format!("{} ", today.slice(lead.start, lead.end)),
        _ => String::new(),
    };
    match (&planned.bounds.body, &item.bounds.body) {
        (Some(wanted), Some(_)) if planned.kind.holds_members() => {
            let members = (plan, planned, wanted.clone());
            with_members((today, among, item), members, indent, read)
        }
        (None, _) if planned.kind == Kind::Fn => {
            read.insert(item.start);
            let signature = today.signature(from(item, planned), item.bounds.stop);
            // A signature ends as the plan ends it: at its `;`, or with
            // nothing, as Go and a script's arrow write it.
            let ends = plan.slice(planned.bounds.stop, planned.bounds.end);
            format!("{indent}{lead}{signature}{ends}")
        }
        _ => {
            read.insert(item.start);
            let whole = today.slice(from(item, planned), item.bounds.end);
            format!("{indent}{lead}{whole}")
        }
    }
}

/// Today's `item`, a block of members, read to the members the plan's
/// `planned` names in `wanted`, under the header of the block that holds
/// them, and closed as the plan closes it.
fn with_members(
    (today, among, item): (&Text, &[Item], &Item),
    (plan, planned, wanted): (&Text, &Item, Range<usize>),
    indent: &str,
    read: &mut BTreeSet<usize>,
) -> String {
    // A type's methods are often split across blocks of one head, and a
    // member is read from whichever holds it.
    let blocks: Vec<&Item> = among.iter().filter(|twin| twin.is(item)).collect();
    let members = today.members(blocks.iter().copied());
    let inner = format!("{indent}    ");
    let mut holding: Vec<&Item> = Vec::new();
    let mut lines = Vec::new();
    for member in plan.inside(planned) {
        let twins = members.iter().filter(|found| found.is(&member));
        let Some(found) = chosen(today, twins, (plan, &member), read) else {
            continue;
        };
        holding.extend(blocks.iter().find(|block| block.holds(found)).copied());
        lines.push(reading(
            (today, &members, found),
            (plan, &member),
            &inner,
            read,
        ));
    }
    // The header is a block's that holds what was read: the plan's own
    // where one of them has it, else the first member's. A plan that
    // moves a member out of its block reads the header it leaves.
    let header = holding
        .iter()
        .find(|block| today.header(block) == plan.header(planned))
        .or(holding.first())
        .map_or(item, |block| *block);
    read.insert(header.start);
    let open = today.slice(from(header, planned), header.bounds.stop + 1);
    // A block closes as the plan closes it: with its `}`, or with nothing
    // where its body ends with its lines, as a Python class does.
    let close = plan.slice(wanted.end, planned.bounds.end).trim();
    std::iter::once(format!("{indent}{open}"))
        .chain(lines)
        .chain((!close.is_empty()).then(|| format!("{indent}{close}")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Where today's `item` is read from for the plan's `planned`: at its
/// attributes where the plan writes some, at its words otherwise.
fn from(item: &Item, planned: &Item) -> usize {
    match planned.start < planned.head {
        true => item.start,
        false => item.head,
    }
}

/// Of `twins`, today's items of the kind and name `planned` has, the one
/// it is read against: the first whose attributes are the plan's, where
/// the plan writes some; else the first that no item planned before it
/// read, so a `cfg` pair planned whole reads each twin once; else the
/// first.
fn chosen<'i>(
    today: &Text,
    twins: impl Iterator<Item = &'i Item>,
    (plan, planned): (&Text, &Item),
    read: &BTreeSet<usize>,
) -> Option<&'i Item> {
    let attributes = plan.attributes(planned);
    twins.min_by_key(|twin| {
        let named = !attributes.is_empty() && today.attributes(twin) == attributes;
        (!named, read.contains(&twin.start))
    })
}

/// What a file holds, split for the search: its items, the members of its
/// `impl`s, `trait`s, classes and interfaces, and the items of its inline
/// modules but a test's.
struct Held {
    items: Vec<Item>,
    members: Vec<Item>,
    nested: Vec<Item>,
}

impl Held {
    fn of(text: &Text) -> Held {
        let items = text.items();
        let members = text.members(items.iter().filter(|item| {
            matches!(
                item.kind,
                Kind::Impl | Kind::Trait | Kind::Class | Kind::Interface
            )
        }));
        // A test's module holds the test's own helpers: a shape is never
        // one of them.
        let searched =
            |item: &&Item| item.kind == Kind::Mod && !text.attributes(item).contains("cfg(test)");
        let nested = text.members(items.iter().filter(searched));
        Held {
            items,
            members,
            nested,
        }
    }

    /// The item `planned` is read against, with the items it stands among:
    /// one of its kind and name at the file's top; for a function, then a
    /// method of its name in a block of methods, and only there for one
    /// that takes `self`; then one of its kind and name in an inline
    /// module. Of twins, such as a `cfg` pair, the one [`chosen`] picks.
    fn found(
        &self,
        today: &Text,
        planned: (&Text, &Item),
        read: &BTreeSet<usize>,
    ) -> Option<(&[Item], &Item)> {
        let (_, wanted) = planned;
        let searched: [&[Item]; 3] = match wanted.kind {
            Kind::Fn if wanted.receiver => [&[], &self.members, &[]],
            Kind::Fn => [&self.items, &self.members, &self.nested],
            _ => [&self.items, &[], &self.nested],
        };
        searched.into_iter().find_map(|among| {
            let twins = among.iter().filter(|item| item.is(wanted));
            Some((among, chosen(today, twins, planned, read)?))
        })
    }
}

#[cfg(test)]
mod tests;
