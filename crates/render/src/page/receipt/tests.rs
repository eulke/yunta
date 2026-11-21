//! The receipt's rows: a printed receipt's criteria row, and the rows, the gist and the
//! verdicts a run's screen says, each as its pane draws them.

use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::events::{ChangesCountedPayload, TerminalState, TokenUsage};
use yunta_core::receipt::{
    BaselineSummary, CostSummary, CriteriaSummary, CriterionEntry, EventChainStatus,
    FindingsSummary, OpenedPullRequest, Receipt, ScopeSummary,
};
use yunta_core::RunId;

use crate::page::Page;

/// A receipt whose one task's criterion was red before the work and is green after it.
fn proved() -> Receipt {
    let entry = CriterionEntry {
        task_id: "global-pack-install".to_string(),
        cmd: "cargo test --workspace --test pack_global_spec".to_string(),
        exit_code: 0,
    };
    Receipt {
        schema_version: Receipt::SCHEMA_VERSION,
        run_id: RunId::from_static("01M7DES1GNED00000000Q78GRR"),
        workflow: "fragua".into(),
        mode: "standard".into(),
        terminal_state: Some(TerminalState::Done),
        criteria: CriteriaSummary {
            total: 1,
            green: 1,
            tasks: 1,
            proving: 1,
            proved: 1,
            red_before: 1,
            guards: 0,
            guards_green: 0,
            entries: vec![entry],
        },
        baseline: None,
        scope: ScopeSummary {
            files_touched: 0,
            violations: Vec::new(),
            granted: 0,
        },
        changed: None,
        runners: Vec::new(),
        cost: CostSummary {
            tokens: TokenUsage::default(),
            cptv: None,
            reroutes: 0,
        },
        event_chain: EventChainStatus::Intact { events: 276 },
        unknown_kinds: Vec::new(),
        diagnostics: Vec::new(),
        failed: Vec::new(),
        blocking_findings: 0,
        findings: FindingsSummary {
            posted: 0,
            answered: 0,
        },
        pull_request: None,
    }
}

/// The receipt's criteria row counts the task's criteria, and says they were red before the
/// work.
#[test]
fn the_receipt_s_criteria_row_says_its_criteria_were_red_before_the_work() {
    let doc = crate::page::Receipt { receipt: &proved() }.document(&Look::plain());
    let drawn = Terminal::on(Look::plain()).draw(&doc);
    let row = "criteria     1 of 1 green, red before the work";
    assert!(drawn.contains(row), "{drawn}");
}

/// The design's run's receipt: its task's criterion red before the work and green after it
/// beside two guards, two of the seven files it touched granted by a person, its four
/// findings answered, its pull request opened.
fn shipped() -> Receipt {
    let mut receipt = proved();
    receipt.criteria.total = 3;
    receipt.criteria.green = 3;
    receipt.criteria.guards = 2;
    receipt.criteria.guards_green = 2;
    receipt.baseline = Some(BaselineSummary {
        suite: "cargo test --workspace".to_string(),
        hash: yunta_core::sha256_hex(b"cargo test --workspace"),
        compared: 1,
        regressions: 0,
        origin: yunta_core::events::BaselineOrigin::Measured,
        red: None,
    });
    receipt.scope.files_touched = 7;
    receipt.scope.granted = 2;
    receipt.findings = FindingsSummary {
        posted: 4,
        answered: 4,
    };
    receipt.cost = CostSummary {
        tokens: TokenUsage {
            input: 26_100_000,
            output: 1_000_000,
            cached: None,
        },
        cptv: Some(27_100_000.0),
        reroutes: 1,
    };
    receipt.changed = Some(ChangesCountedPayload {
        tree: "81e6bb96".into(),
        files: 7,
        added: 65,
        removed: 24,
        largest: Vec::new(),
    });
    let url = "https://github.com/eulke/yunta/pull/31".to_string();
    receipt.pull_request = Some(OpenedPullRequest { number: 31, url });
    receipt
}

/// What a run's screen says of `receipt` in its pane: its checks, then its facts.
fn on_screen(receipt: &Receipt) -> String {
    let look = Look {
        glyphs: surco::glyphs::Glyphs::Unicode,
        ..Look::plain()
    };
    let page = crate::page::Receipt { receipt };
    let told = crate::page::Told::OnScreen(look.glyphs);
    let doc = surco::doc::Doc::new()
        .with(page.checks(told))
        .with(page.facts(told));
    Terminal::on(look).draw(&doc)
}

/// A run's screen says the receipt's rows in the design's words, its checks then what it
/// spent, changed and opened, and its gist under its ribbon.
#[test]
fn the_receipt_s_rows_on_a_run_s_screen_read_as_its_pane_says_them() {
    let rows = [
        "  ✓ criteria     1 of 1 green, red before the work",
        "  ✓ baseline     cargo test --workspace still passes",
        "  ✓ scope        7 files, 2 of them granted by you",
        "  ✓ review       4 findings, 4 answered",
        "  ✓ event chain  intact",
        "",
        "  cost         27.1M tokens per verified task, 1 reroute",
        "  changed      7 files, +65 −24",
        "  pull request #31: https://github.com/eulke/yunta/pull/31",
    ];
    let receipt = shipped();
    let drawn = on_screen(&receipt);
    assert!(drawn.contains(&rows.join("\n")), "{drawn}");
    let page = crate::page::Receipt { receipt: &receipt };
    let gist = "every criterion green, every finding answered, the log's chain intact";
    assert_eq!(page.gist().as_deref(), Some(gist));
}

/// A gate that ships the work says its findings and its criteria in one line, under the done
/// mark where each finding is answered and each criterion green, under the caution mark where
/// one is not.
#[test]
fn a_gate_that_ships_the_work_says_its_verdicts_in_one_line() {
    let receipt = shipped();
    let glyphs = surco::glyphs::Glyphs::Unicode;
    let line = crate::page::verdicts(&receipt.findings, &receipt.criteria, glyphs);
    assert_eq!(
        line.text(),
        "✓ 4 findings, 4 answered · 1 task, 1 criterion green"
    );
    let open = FindingsSummary {
        posted: 4,
        answered: 3,
    };
    let line = crate::page::verdicts(&open, &receipt.criteria, glyphs);
    assert_eq!(
        line.text(),
        "▲ 4 findings, 3 answered · 1 task, 1 criterion green"
    );
}

/// A run that finished holding a blocking finding says its rows as its screen does, and no
/// gist of the receipt's own.
#[test]
fn a_reported_receipt_s_rows_read_as_its_pane_says_them() {
    let mut receipt = proved();
    receipt.criteria = CriteriaSummary {
        total: 0,
        green: 0,
        tasks: 0,
        proving: 0,
        proved: 0,
        red_before: 0,
        guards: 0,
        guards_green: 0,
        entries: Vec::new(),
    };
    receipt.findings.posted = 1;
    receipt.blocking_findings = 1;
    let rows = [
        "  ◌ criteria     none declared, nothing to prove",
        "  ◌ baseline     not used by this workflow",
        "  ✓ scope        no file touched",
        "  ✗ review       1 finding, 0 answered",
        "  ✓ event chain  intact",
        "",
        "  cost         nothing spent",
    ];
    let drawn = on_screen(&receipt);
    assert!(drawn.contains(&rows.join("\n")), "{drawn}");
    assert_eq!(crate::page::Receipt { receipt: &receipt }.gist(), None);
}

/// A printed receipt's baseline row says the suite still passes, the command in a code span
/// as a printed page sets a command, then what the comparisons found.
#[test]
fn a_printed_receipt_s_baseline_row_sets_the_suite_as_a_command() {
    let look = Look {
        glyphs: surco::glyphs::Glyphs::Unicode,
        width: surco::look::Width::exact(160),
        ..Look::plain()
    };
    let doc = crate::page::Receipt {
        receipt: &shipped(),
    }
    .document(&look);
    let drawn = Terminal::on(look).draw(&doc);
    let row = "`cargo test --workspace` still passes · 0 regressions across 1 comparison";
    assert!(drawn.contains(row), "{drawn}");
}
