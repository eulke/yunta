use yunta_core::caution::Caution;

use crate::caution::line;

#[test]
fn the_budget_caution_keeps_the_words_the_warning_has_always_had() {
    assert_eq!(
        line(&Caution::BudgetBelowHistory {
            cap: 250,
            p90: 300.0,
            samples: 3
        }),
        "`limits.max_tokens_per_run` (250) is below this workflow's historical p90 (300 tokens \
         over 3 runs) — the run may pause on its budget"
    );
}
