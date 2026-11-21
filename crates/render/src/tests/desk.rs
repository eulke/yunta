//! A plan as its desk reads it: every place a person can stand on, what each shows, and
//! what the rail says of the whole, from the plan the rest of these tests read.

use surco::components::{Desk, DeskFoot, FootRow, Legend, RailGroup, RailRow};
use surco::glyphs::Glyphs;
use surco::golden::assert_golden;
use surco::line::Line;
use surco::look::{Look, Width};
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_core::shown::{HeldTo, JudgedCriterion, PlanReview, TaskReview};
use yunta_core::{SpecFile, TasksFile};

use super::documents::{global_packs, goldens, review as greeting};
use crate::page::desk::{about, Reading, Spot, View};

mod rail;

fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: surco::motion::Motion::Moves,
    }
}

/// What a person spot on `spot` reads in the pane of a terminal 80 cells wide, laid out at
/// the pane's measure as the desk lays it out.
fn read(reading: &Reading, spot: &Spot) -> Vec<String> {
    let terminal = look(80);
    let measure = Look {
        width: Width::row(Desk::pane_cells(&terminal), None),
        ..terminal
    };
    Terminal::on(measure)
        .lines(&reading.pane_doc(spot))
        .iter()
        .map(Line::text)
        .collect()
}

fn holds(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|row| row.contains(needle))
}

/// The rail as a person spot on `at` reads it.
fn rail(reading: &Reading, at: usize) -> Vec<RailGroup> {
    reading.rail(&Spot {
        at,
        view: View::Stop,
    })
}

/// A rail's groups by what each opens with, and each stop as `lead name`.
fn stops(groups: &[RailGroup]) -> Vec<(String, String, bool)> {
    groups
        .iter()
        .flat_map(|group| &group.rows)
        .filter_map(|row| match row {
            RailRow::Stop {
                lead,
                name,
                pointed,
            } => Some((lead.clone(), name.clone(), *pointed)),
            _ => None,
        })
        .collect()
}

/// The plan the design's run hands its gate, from the documents its kept run holds: one
/// task held to the spec's one test, three decisions made for the person, two risks and
/// three things it leaves out, and nothing that keeps it from being proven.
fn designed() -> PlanReview {
    let plan: TasksFile = yunta_core::yaml::parse(include_str!(
        "../../../testkit-ui/fixtures/designed/run/artifacts/plan/tasks.yaml"
    ))
    .unwrap();
    let spec: SpecFile = yunta_core::yaml::parse(include_str!(
        "../../../testkit-ui/fixtures/designed/run/artifacts/spec/spec.yaml"
    ))
    .unwrap();
    let test = &spec.of(&plan.tasks[0].id).expect("its spec").tests[0];
    let task = TaskReview {
        task: plan.tasks[0].id.clone(),
        criteria: vec![JudgedCriterion {
            cmd: test.cmd.clone(),
            proves: Some(test.proves.clone()),
            from: HeldTo::Spec,
        }],
        denied: Vec::new(),
        guards: Vec::new(),
    };
    PlanReview {
        plan,
        departed: Vec::new(),
        spec: Some(spec),
        spec_node: Some("spec".into()),
        held: Vec::new(),
        tasks: vec![task],
        handed_over: None,
        answered: Vec::new(),
        asked: Some("add support for installing a pack globally".into()),
        shapes_read: None,
    }
}

/// What the plan says of itself is its description and its design, with its drawings.
#[test]
fn the_change_is_what_the_author_wrote() {
    let review = greeting();
    let reading = Reading::of(review.clone());
    let spot = Spot {
        at: 0,
        view: View::Stop,
    };
    let rows = read(&reading, &spot);
    assert!(
        holds(&rows, "The program says nothing today."),
        "{}",
        rows.join("\n")
    );
    assert!(holds(&rows, "owns the words"), "{}", rows.join("\n"));
    assert!(
        !holds(&rows, "What does it say"),
        "what it decided is a key away: {}",
        rows.join("\n")
    );
}

/// Whether one of `rows` opens with `label`, as a field's label stands at its row's start.
fn labelled(rows: &[String], label: &str) -> bool {
    rows.iter().any(|row| row.trim_start().starts_with(label))
}

/// A card has the change, the files it changes heading their code and no row of their own;
/// its tests are a key away.
#[test]
fn a_card_has_the_change_and_no_row_of_its_files_or_its_tests() {
    let reading = Reading::of(global_packs());
    let spot = Spot {
        at: 1,
        view: View::Stop,
    };
    let card = read(&reading, &spot);
    assert!(holds(&card, "you will see"), "{}", card.join("\n"));
    assert!(holds(&card, "PackInstallScope"), "{}", card.join("\n"));
    assert!(
        !holds(&card, "global_pack_storage.rs"),
        "{}",
        card.join("\n")
    );
    for said_elsewhere in ["changes", "tested by"] {
        assert!(!labelled(&card, said_elsewhere), "{}", card.join("\n"));
    }
}

/// A task's proof says each of its tests, what it proves and the command that runs it, with
/// the file the spec wrote whole under its path; the suite every task keeps is the whole
/// plan's proof's to say.
#[test]
fn a_tasks_proof_has_its_tests_whole_and_the_plans_has_the_suite() {
    let reading = Reading::of(global_packs());
    let proof = read(
        &reading,
        &Spot {
            at: 1,
            view: View::Proof,
        },
    );
    assert!(labelled(&proof, "proves"), "{}", proof.join("\n"));
    assert!(labelled(&proof, "runs"), "{}", proof.join("\n"));
    assert!(!labelled(&proof, "the suite"), "{}", proof.join("\n"));
    for said in [
        "global_pack_storage.rs",
        "pack::installed_packs(pack::PackInstallScope::Project",
    ] {
        assert!(holds(&proof, said), "{said}: {}", proof.join("\n"));
    }
    for unsaid in ["written by the spec", "PackInstallScope {"] {
        assert!(!holds(&proof, unsaid), "{unsaid}: {}", proof.join("\n"));
    }
    let whole = read(
        &reading,
        &Spot {
            at: 0,
            view: View::Proof,
        },
    );
    assert!(holds(&whole, "the suite"), "{}", whole.join("\n"));
    assert!(holds(&whole, "cargo clippy"), "{}", whole.join("\n"));
}

/// What was decided is every decision the plan made, each whole: its question, then what the
/// plan chose and why under it, a decision that restates what the person answered among
/// them; no pointer, no number, and nothing the rail counts beside them.
#[test]
fn what_was_decided_shows_every_decision_whole() {
    let review = greeting();
    let reading = Reading::of(review.clone());
    let spot = Spot {
        at: 0,
        view: View::Choices,
    };
    let rows = read(&reading, &spot);
    let first = rows.iter().position(|row| !row.trim().is_empty());
    let opening = first.map(|at| rows[at].as_str());
    assert_eq!(
        opening,
        Some("What does it say when no name is given?"),
        "{}",
        rows.join("\n")
    );
    assert!(holds(&rows, "  It greets `world`."), "{}", rows.join("\n"));
    assert!(
        holds(&rows, "should still say something"),
        "{}",
        rows.join("\n")
    );
    assert!(
        rows.iter().any(|row| row == "Which word greets?"),
        "the decision that restates an answer: {}",
        rows.join("\n")
    );
    for counted_on_the_rail in ["you answered", "risks", "left out", "▸", "Refuse to run"] {
        assert!(
            !holds(&rows, counted_on_the_rail),
            "{counted_on_the_rail}: {}",
            rows.join("\n")
        );
    }

    let packs = global_packs();
    let reading = Reading::of(packs.clone());
    let rows = read(&reading, &spot);
    let questions = packs
        .plan
        .decisions
        .iter()
        .map(|decision| &decision.question);
    for question in questions {
        // A question wider than the pane goes on in the rows under it.
        let opens = |row: &String| !row.trim().is_empty() && question.starts_with(row.as_str());
        assert!(rows.iter().any(opens), "{question}: {}", rows.join("\n"));
    }
}

/// What a decision's card says of the plan: what it is about, unless the question says
/// it, and how big it is.
#[test]
fn a_card_says_what_the_plan_is_about_and_how_big_it_is() {
    let review = global_packs();
    let said = about(
        &review,
        "Plan and its tests registered. Approve?",
        Glyphs::Ascii,
    );
    assert_eq!(
        said,
        [
            "Install packs in the user's Yunta state directory for use across projects",
            "4 tasks in 3 steps ; 4 decided for you ; 2 risks ; 2 left out",
            "what it changes is named in 5 files outside every task",
        ]
    );
    let said = about(
        &review,
        "Approve the plan: Install packs in the user's Yunta state directory for use across projects.?",
        Glyphs::Ascii,
    );
    assert_eq!(
        said.len(),
        2,
        "the question already said it: {}",
        said.join("\n")
    );
}

/// The design's plan at its desk on a terminal 120 columns wide, between the rule and the
/// attention row, its keys in the screen's legend: the rail's first ten rows, its places
/// and what it says, each row to the rule beside the pane.
#[test]
fn the_designs_plan_draws_its_rail_at_120_columns() {
    let reading = Reading::of(designed());
    let foot = DeskFoot {
        question: "Approve the plan: Add explicit per-user global pack installation alongside \
                   project-level installs?"
            .to_string(),
        row: FootRow::Options {
            named: ["approve", "adjust", "abort"].map(str::to_string).to_vec(),
            more: 0,
        },
        legend: Legend::new(),
    };
    let look = look(118);
    let spot = Spot {
        at: 1,
        view: View::Stop,
    };
    let title = ["plan", "Q78GRR", "fragua"].map(str::to_string);
    let edge = Some("needs you  approve-plan".to_string());
    let rule = look.glyphs.gutter();
    let rail: String = reading
        .desk(&spot, 0, &title, edge, foot, &look)
        .drawn(&look, 32)
        .lines
        .iter()
        .map(Line::text)
        .filter_map(|row| Some(format!("{}\n", &row[..row.find(rule)? + rule.len_utf8()])))
        .take(10)
        .collect();
    assert_golden(&goldens().join("desk-designed-rail.txt"), &rail);
}

/// The whole desk over the plan these tests read, at the size a person reads it at, for
/// each thing a person can have in front of them.
#[test]
fn the_desk_matches_its_goldens() {
    let review: PlanReview = global_packs();
    let reading = Reading::of(review.clone());
    let foot = |[moves, p, d]: [&str; 3]| DeskFoot {
        question: "Plan and its tests registered. Approve?".to_string(),
        row: FootRow::Options {
            named: vec![
                "approve".to_string(),
                "adjust".to_string(),
                "abort".to_string(),
            ],
            more: 0,
        },
        legend: Legend::new()
            .key("j k", moves)
            .key("p", p)
            .key("d", d)
            .key("esc", "back"),
    };
    let reading_keys = ["task", "proof", "decided for you"];
    for (name, at, view, keys) in [
        ("desk-the-change", 0, View::Stop, reading_keys),
        ("desk-task", 2, View::Stop, reading_keys),
        (
            "desk-proof",
            1,
            View::Proof,
            ["task", "the card", "decided for you"],
        ),
        (
            "desk-choices",
            1,
            View::Choices,
            ["task", "proof", "the tasks"],
        ),
    ] {
        let look = look(118);
        let spot = Spot { at, view };
        let laid = reading
            .desk(
                &spot,
                0,
                &["plan", "EKPBGE", "fragua"].map(str::to_string),
                Some("needs you  approve-plan".to_string()),
                foot(keys),
                &look,
            )
            // A terminal 36 rows high holds the desk over its last row.
            .drawn(&look, 35);
        let text: String = laid
            .lines
            .iter()
            .map(|line| format!("{}\n", line.text().trim_end()))
            .collect();
        assert_golden(&goldens().join(format!("{name}.txt")), &text);
    }
}

/// The design's plan at its desk on a terminal of 80 columns, where no rail stands: the
/// pane's title row, under the desk's head and a blank row, says at its far end where the
/// pane stands among the places a person stands on, then what its edge says, as row 7 of
/// the targets `desk` and `desk-change` does.
#[test]
fn with_no_rail_the_pane_says_its_place_among_the_places() {
    let reading = Reading::of(designed());
    let foot = || DeskFoot {
        question: "Approve the plan?".to_string(),
        row: FootRow::Options {
            named: ["approve", "adjust", "abort"].map(str::to_string).to_vec(),
            more: 0,
        },
        legend: Legend::new(),
    };
    let look = look(78);
    let title = ["plan", "Q78GRR", "fragua"].map(str::to_string);
    let title_row = |at: usize| {
        let spot = Spot {
            at,
            view: View::Stop,
        };
        let laid = reading
            .desk(&spot, 0, &title, None, foot(), &look)
            .drawn(&look, 20);
        let rows: Vec<String> = laid.lines.iter().map(Line::text).collect();
        let under = rows.iter().position(|row| row.trim().is_empty());
        under
            .and_then(|blank| rows.get(blank + 1))
            .map(|row| row.trim_end().to_string())
            .unwrap_or_default()
    };
    let task = title_row(1);
    assert!(task.ends_with("  2 of 2 · step 1 of 1"), "{task}");
    let change = title_row(0);
    assert!(change.trim_start().starts_with("the change "), "{change}");
    assert!(change.ends_with("  1 of 2 · 1 task"), "{change}");
}
