use sparrow::multiboard::{MultiBoardOptions, solve};
use sparrow::util::multiboard_io::{MultiBoardInput, StopReason, output_directory};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn geometric_rejection_retries_without_consuming_inventory() {
    let mut input = input();
    input.items[0].demand = 1;
    input.boards.truncate(1);
    input.boards[0].length = 3.0;
    input.boards[0].width = 3.0;
    let dir = directory();
    let summary = solve(&input, &options(), &dir).unwrap();
    assert!(matches!(
        summary.stop_reason,
        StopReason::TimeBudgetExhausted
    ));
    assert!(summary.candidate_attempts >= 2);
    assert_eq!(
        summary.failed_candidate_attempts,
        summary.candidate_attempts
    );
    assert_eq!(summary.inventory[0].used, 0);
    assert_eq!(summary.unplaced[0].quantity, 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn interrupted_and_zero_demand_jobs() {
    let mut input = input();
    let options = options();
    options.interrupted.store(true, Ordering::Relaxed);
    let dir = directory();
    let summary = solve(&input, &options, &dir).unwrap();
    assert!(matches!(summary.stop_reason, StopReason::Interrupted));
    assert_eq!(summary.unplaced[0].quantity, 4);
    std::fs::remove_dir_all(dir).unwrap();
    input.items[0].demand = 0;
    let dir = directory();
    let summary = solve(&input, &options, &dir).unwrap();
    assert!(summary.all_items_placed);
    assert!(summary.unplaced.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

fn directory() -> PathBuf {
    std::env::temp_dir().join(format!(
        "sparrow-multiboard-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn input() -> MultiBoardInput {
    serde_json::from_value(serde_json::json!({
        "name": "../unsafe/name", "min_item_separation": 0.05,
        "boards": [
            {"id":"low", "length":5,"width":4,"quantity":3,"priority":1},
            {"id":"high", "length":5,"width":4,"quantity":2,"priority":10},
            {"id":"equal", "length":5,"width":4,"quantity":1,"priority":10}
        ],
        "items":[{"id":901,"demand":4,"orientation":{"rotation":{"mode":"discrete","angles":[0]}},
            "shape":{"type":"rectangle","data":{"x_min":10,"y_min":-7,"width":3,"height":3}}}]
    }))
    .unwrap()
}
fn options() -> MultiBoardOptions {
    MultiBoardOptions {
        time_limit: Duration::from_millis(800),
        seed: 42,
        workers: Some(1),
        ..MultiBoardOptions::default()
    }
}

#[test]
fn priorities_inventory_conservation_and_external_transforms() {
    let mut input = input();
    for board in &mut input.boards {
        board.length = 4.0;
    }
    let dir = directory();
    let mut options = options();
    options.time_limit = Duration::from_secs(5);
    let summary = solve(&input, &options, &dir).unwrap();
    assert!(summary.all_items_placed);
    assert_eq!(
        summary.inventory.iter().map(|b| b.used).collect::<Vec<_>>(),
        [1, 2, 1]
    );
    assert_eq!(
        summary
            .inventory
            .iter()
            .map(|b| b.utilization_weight)
            .collect::<Vec<_>>(),
        [1, 2, 2]
    );
    assert!(summary.unplaced.is_empty());
    for (idx, board) in summary.boards.iter().enumerate() {
        assert_eq!(board.instance_id, format!("board_{:06}", idx + 1));
        assert_eq!(board.layout.container_id, idx as u64 + 1);
        assert_eq!(board.layout.placed_items.len(), 1);
        assert!((board.utilization - 9.0 / 16.0).abs() < 1e-6);
        let item = &board.layout.placed_items[0];
        assert_eq!(item.item_id, 901);
        let (tx, ty) = item.transformation.translation;
        assert!(item.transformation.rotation.abs() < 1e-4);
        assert!(10.0 + tx >= 0.0 && 13.0 + tx <= board.length);
        assert!(-7.0 + ty >= 0.0 && -4.0 + ty <= board.width);
        let svg = std::fs::read_to_string(dir.join(&board.svg)).unwrap();
        assert!(svg.contains("container_"));
    }
    assert_eq!(summary.inventory.iter().map(|b| b.used).sum::<u64>(), 4);
    let mut item_area = 0.0;
    let mut board_area = 0.0;
    let mut weighted_item_area = 0.0;
    let mut weighted_board_area = 0.0;
    for board in &summary.boards {
        let source = input
            .boards
            .iter()
            .find(|b| b.id == board.board_id)
            .unwrap();
        let weight = if source.priority == 10 { 2 } else { 1 };
        assert_eq!(board.utilization_weight, weight);
        let area = board.layout.placed_items.len() as f64 * 9.0;
        let stock_area = f64::from(source.length) * f64::from(source.width);
        assert!((board.used_item_area - area).abs() < 1e-9);
        item_area += area;
        board_area += stock_area;
        weighted_item_area += weight as f64 * area;
        weighted_board_area += weight as f64 * stock_area;
    }
    assert!((summary.overall_utilization - item_area / board_area).abs() < 1e-12);
    assert!(
        (summary.weighted_utilization - weighted_item_area / weighted_board_area).abs() < 1e-12
    );
    let json: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(dir.join("summary.json")).unwrap()).unwrap();
    assert_eq!(
        json["weighted_utilization"].as_f64().unwrap(),
        summary.weighted_utilization
    );
    assert!(dir.join("summary.json").is_file());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn different_dimensions_skip_unfound_high_board() {
    let mut input = input();
    input.items[0].demand = 1;
    input.boards.truncate(2);
    input.boards[1].length = 2.0;
    input.boards[1].width = 2.0;
    let dir = directory();
    let summary = solve(&input, &options(), &dir).unwrap();
    assert_eq!(summary.boards.len(), 1);
    assert_eq!(summary.boards[0].board_id, "low");
    assert_eq!(summary.inventory[1].used, 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn zero_budget_preserves_demand() {
    let input = input();
    let dir = directory();
    let mut options = options();
    options.time_limit = Duration::ZERO;
    let summary = solve(&input, &options, &dir).unwrap();
    assert!(matches!(
        summary.stop_reason,
        StopReason::TimeBudgetExhausted
    ));
    assert!(summary.boards.is_empty());
    assert_eq!(summary.unplaced[0].quantity, 4);
    assert_eq!(summary.candidate_attempts, 0);
    assert_eq!(summary.overall_utilization, 0.0);
    assert_eq!(summary.weighted_utilization, 0.0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn finite_inventory_and_unfound_are_not_infeasibility_claims() {
    let mut input = input();
    input.boards.truncate(1);
    input.boards[0].quantity = 1;
    let dir = directory();
    let summary = solve(&input, &options(), &dir).unwrap();
    assert!(matches!(
        summary.stop_reason,
        StopReason::TimeBudgetExhausted
    ));
    assert_eq!(summary.unplaced[0].quantity, 3);
    std::fs::remove_dir_all(dir).unwrap();
    input.boards[0].length = 2.0;
    input.boards[0].width = 2.0;
    let dir = directory();
    let summary = solve(&input, &options(), &dir).unwrap();
    assert!(matches!(
        summary.stop_reason,
        StopReason::TimeBudgetExhausted
    ));
    assert_eq!(summary.unplaced[0].quantity, 4);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn validation_and_safe_paths() {
    let mut input = input();
    assert_eq!(
        output_directory(Path::new("output"), &input.name).parent(),
        Some(Path::new("output"))
    );
    input.boards[1].id = input.boards[0].id.clone();
    assert!(input.validate().is_err());
    input = self::input();
    input.items.push(input.items[0].clone());
    assert!(input.validate().is_err());
    input = self::input();
    input.boards[0].length = f32::NAN;
    assert!(input.validate().is_err());
    input = self::input();
    input.min_item_separation = -1.0;
    assert!(input.validate().is_err());
}

#[test]
fn completed_jobs_keep_searching_and_preserve_quantities() {
    let mut input = input();
    input.items[0].demand = 1;
    let mut options = options();
    options.time_limit = Duration::from_millis(300);
    let dir = directory();
    let summary = solve(&input, &options, &dir).unwrap();
    assert!(summary.all_items_placed);
    assert!(summary.elapsed_seconds >= options.time_limit.as_secs_f64());
    assert!(summary.search_rounds >= 2);
    assert!((1..=summary.search_rounds).contains(&summary.best_round));
    assert!(matches!(
        summary.stop_reason,
        StopReason::TimeBudgetExhausted
    ));
    let placed: u64 = summary
        .boards
        .iter()
        .flat_map(|b| &b.items)
        .map(|i| i.quantity)
        .sum();
    let unplaced: u64 = summary.unplaced.iter().map(|i| i.quantity).sum();
    assert_eq!(placed + unplaced, input.items[0].demand);
    for board in &summary.inventory {
        assert_eq!(board.used + board.remaining, board.available);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn weighted_search_can_replace_an_inefficient_high_priority_board() {
    let mut input = input();
    input.items[0].demand = 1;
    input.boards.truncate(2);
    input.boards[0].length = 4.0;
    input.boards[0].width = 4.0;
    input.boards[1].length = 100.0;
    input.boards[1].width = 100.0;
    let dir = directory();
    let summary = solve(&input, &options(), &dir).unwrap();
    assert!(summary.all_items_placed);
    assert!(summary.search_rounds >= 2);
    assert_eq!(summary.boards.len(), 1);
    assert_eq!(summary.boards[0].board_id, "low");
    assert_eq!(summary.boards[0].utilization_weight, 1);
    assert_eq!(summary.inventory[1].utilization_weight, 2);
    assert_eq!(summary.inventory[1].used, 0);
    assert!((summary.overall_utilization - 9.0 / 16.0).abs() < 1e-12);
    assert!((summary.weighted_utilization - 9.0 / 16.0).abs() < 1e-12);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn no_inventory_needs_no_search() {
    let mut input = input();
    input.boards.clear();
    let dir = directory();
    let summary = solve(&input, &options(), &dir).unwrap();
    assert_eq!(summary.search_rounds, 0);
    assert_eq!(summary.best_round, 0);
    assert!(!summary.all_items_placed);
    assert_eq!(summary.unplaced[0].quantity, input.items[0].demand);
    assert!(matches!(
        summary.stop_reason,
        StopReason::InventoryExhausted
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(feature = "live_svg")]
#[test]
fn live_svg_contains_all_best_boards_and_unique_references() {
    use std::collections::HashSet;
    use svg::parser::Event;
    let mut input = input();
    input.name = "Job <&>".to_owned();
    input.boards[1].id = "high <&> \"board\"".to_owned();
    input.items[0].demand = 2;
    let dir = directory();
    std::fs::create_dir_all(&dir).unwrap();
    let live = dir.join("live.svg");
    std::fs::write(&live, "old preview").unwrap();
    let mut options = options();
    options.time_limit = Duration::from_secs(5);
    options.live_svg_path = Some(live.clone());
    let summary = solve(&input, &options, &dir).unwrap();
    assert_eq!(summary.boards.len(), 2);
    let text = std::fs::read_to_string(&live).unwrap();
    assert!(!text.contains("old preview"));
    assert!(text.contains("Current global best | Job &lt;&amp;&gt;"));
    assert!(text.contains("Placed 2/2"));
    assert!(text.contains(&format!("Best round {}", summary.best_round)));
    assert!(text.contains("Status: final: time_budget_exhausted"));
    for board in &summary.boards {
        let escaped_id = board
            .board_id
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        assert!(text.contains(&format!(
            "{} | Board type: {}",
            board.instance_id, escaped_id
        )));
        assert!(text.contains(&format!("Utilization weight: {}", board.utilization_weight)));
    }
    assert!(text.contains(&format!(
        "Overall utilization: {:.2}% | Weighted utilization: {:.2}%",
        summary.overall_utilization * 100.0,
        summary.weighted_utilization * 100.0
    )));
    let mut ids = HashSet::new();
    let mut references = Vec::new();
    for event in svg::read(&text).unwrap() {
        match event {
            Event::Tag(_, _, attributes) => {
                if let Some(id) = attributes.get("id") {
                    assert!(ids.insert(id.to_string()), "duplicate SVG id {id}");
                }
                if let Some(href) = attributes.get("href") {
                    references.push(href.to_string());
                }
            }
            Event::Error(error) => panic!("invalid SVG: {error}"),
            _ => {}
        }
    }
    assert!(!references.is_empty());
    for reference in references {
        assert!(ids.contains(reference.strip_prefix('#').unwrap()));
    }
    for number in 1..=2 {
        assert!(text.contains(&format!("board_{number:06} | Board type:")));
        assert!(ids.contains(&format!("live_board_{number}_item_{number}_901")));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(feature = "live_svg")]
#[test]
fn live_svg_write_failure_has_destination_context() {
    let dir = directory();
    let mut options = options();
    options.live_svg_path = Some(dir.join("missing").join("live.svg"));
    let error = solve(&input(), &options, &dir).err().unwrap();
    assert!(
        error
            .to_string()
            .contains("failed to write multiboard live SVG to")
    );
    assert!(error.to_string().contains("live.svg"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(feature = "live_svg"))]
#[test]
fn live_svg_path_is_ignored_without_feature() {
    let dir = directory();
    let live = dir.join("live.svg");
    let mut options = options();
    options.time_limit = Duration::ZERO;
    options.live_svg_path = Some(live.clone());
    solve(&input(), &options, &dir).unwrap();
    assert!(!live.exists());
    std::fs::write(&live, "existing preview").unwrap();
    solve(&input(), &options, &dir).unwrap();
    assert_eq!(std::fs::read_to_string(live).unwrap(), "existing preview");
    std::fs::remove_dir_all(dir).unwrap();
}
