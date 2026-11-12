//! The checks a page lists, each as its row, and the tally its headline makes
//! of them: what `doctor` and `verify` both draw.

use surco::components::{Check, Checklist, Found, Headline};

/// `headline` with how many of `checks` are problems and how many are
/// worth a look.
pub(crate) fn tallied(headline: Headline, checks: &[Check]) -> Headline {
    let count = |found: Found| checks.iter().filter(|check| check.found == found).count();
    headline.tally(count(Found::Problem), count(Found::Caution))
}

/// `checks`, as the checklist a reader sees.
pub(crate) fn checklist(checks: &[Check]) -> Checklist {
    let mut checklist = Checklist::default();
    checklist.extend(checks.iter().cloned());
    checklist
}

#[cfg(test)]
mod tests {
    use surco::components::{Checklist, Found};
    use surco::doc::Doc;
    use surco::look::Look;
    use surco::surface::{Surface, Terminal};

    use super::checklist;

    #[test]
    fn a_page_lists_its_checks_as_the_checklist_they_came_from() {
        let mut checks = Checklist::default();
        checks.push(Found::Holds, "chain", "intact: 3 events verified");
        checks.push(
            Found::Problem,
            "objects",
            "broken: 1 of 2 are not the bytes the run accepted",
        );
        let listed: Vec<_> = checks.checks().cloned().collect();
        let look = Look::plain();
        assert_eq!(
            Terminal::on(look).draw(&Doc::new().with(checklist(&listed))),
            Terminal::on(look).draw(&Doc::new().with(checks))
        );
    }
}
