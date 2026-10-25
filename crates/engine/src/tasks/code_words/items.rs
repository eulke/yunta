//! What a Rust file holds of the items a shape declares, read the way the
//! shape writes them: a function to its signature where the shape stops at
//! one, an `impl`, a `trait` or a module to the members the shape names,
//! and attributes only where the shape writes some.
//!
//! Both texts are split into items the same way, see [`text`]; a file's
//! are searched at its top, one level into each `impl` and `trait`, and
//! one level into each inline module that is not a test's.

mod text;

use std::collections::BTreeSet;
use std::ops::Range;

use text::{Item, Kind, Text};

/// What `source`, a Rust file, holds of the items `planned` declares, each
/// read as `planned` writes it, in its order, joined by a blank line; `None`
/// when it holds none of them.
pub(crate) fn as_planned(source: &str, planned: &str) -> Option<String> {
    let today = Text::of(source);
    let plan = Text::of(planned);
    let held = Held::of(&today);
    // Where each of today's items an earlier planned item read starts.
    let mut read = BTreeSet::new();
    let mut readings = Vec::new();
    for wanted in plan.items(plan.whole()) {
        if let Some((among, found)) = held.found(&today, (&plan, &wanted), &read) {
            let reading = reading((&today, among, found), (&plan, &wanted), "", &mut read);
            readings.push(reading);
        }
    }
    (!readings.is_empty()).then(|| readings.join("\n\n"))
}

/// Today's `item`, one of the items `among`, read the way the plan writes
/// `planned`, its first line set in by `indent`: a function to its
/// signature where the plan ends it at its `;`, an `impl`, a `trait` or a
/// module to the members the plan names, each set in one level further,
/// and any other item whole. Its attributes are read only where the plan
/// writes some. What it reads joins `read`.
fn reading(
    (today, among, item): (&Text, &[Item], &Item),
    (plan, planned): (&Text, &Item),
    indent: &str,
    read: &mut BTreeSet<usize>,
) -> String {
    match (&planned.bounds.body, &item.bounds.body) {
        (Some(wanted), Some(_)) if planned.kind.holds_members() => {
            let members = (plan, planned, wanted.clone());
            with_members((today, among, item), members, indent, read)
        }
        (None, _) if planned.kind == Kind::Fn => {
            read.insert(item.start);
            let signature = today.signature(from(item, planned), item.bounds.stop);
            format!("{indent}{signature};")
        }
        _ => {
            read.insert(item.start);
            let whole = today.slice(from(item, planned), item.bounds.end);
            format!("{indent}{whole}")
        }
    }
}

/// Today's `item`, an `impl`, a `trait` or a module, read to the members
/// the plan's `planned` names in `wanted`, under the header of the block
/// that holds them.
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
    for member in plan.items(wanted) {
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
    let close = format!("{indent}}}");
    std::iter::once(format!("{indent}{open}"))
        .chain(lines)
        .chain(std::iter::once(close))
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
/// `impl`s and `trait`s, and the items of its inline modules but a test's.
struct Held {
    items: Vec<Item>,
    members: Vec<Item>,
    nested: Vec<Item>,
}

impl Held {
    fn of(text: &Text) -> Held {
        let items = text.items(text.whole());
        let members = text.members(
            items
                .iter()
                .filter(|item| matches!(item.kind, Kind::Impl | Kind::Trait)),
        );
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
    /// method of its name in an `impl` or a `trait`, and only there for one
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
