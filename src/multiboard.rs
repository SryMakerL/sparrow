use crate::config::{DEFAULT_SPARROW_CONFIG, SparrowConfig};
use crate::consts::DRAW_OPTIONS;
use crate::optimizer::optimize;
use crate::util::listener::{ReportType, SolutionListener};
use crate::util::multiboard_io::*;
use crate::util::terminator::Terminator;
use anyhow::{Result, ensure};
use jagua_rs::Instant;
use jagua_rs::io::export::export_layout_snapshot;
use jagua_rs::io::import::Importer;
use jagua_rs::io::svg::s_layout_to_svg;
use jagua_rs::probs::spp::entities::{SPProblem, SPSolution};
use jagua_rs::probs::spp::io::{ext_repr::ExtSPInstance, import_instance};
use rand::{Rng, SeedableRng, rngs::Xoshiro256PlusPlus, seq::SliceRandom};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

pub struct MultiBoardOptions {
    pub time_limit: Duration,
    pub seed: u64,
    pub workers: Option<usize>,
    pub interrupted: Arc<AtomicBool>,
    /// Optional live snapshot destination; only used with the `live_svg` feature.
    pub live_svg_path: Option<PathBuf>,
}

impl Default for MultiBoardOptions {
    fn default() -> Self {
        Self {
            time_limit: Duration::from_secs(600),
            seed: rand::random(),
            workers: None,
            interrupted: Arc::new(AtomicBool::new(false)),
            live_svg_path: None,
        }
    }
}

struct FitListener {
    length: f32,
    found: Arc<AtomicBool>,
}

impl SolutionListener for FitListener {
    fn report(&mut self, report: ReportType, solution: &SPSolution) {
        if matches!(
            report,
            ReportType::ExplFeas | ReportType::CmprFeas | ReportType::Final
        ) && solution.strip_width().is_finite()
            && solution.strip_width() <= self.length
        {
            self.found.store(true, Ordering::Relaxed);
        }
    }
}

struct BudgetTerminator {
    deadline: Instant,
    phase_deadline: Instant,
    interrupted: Arc<AtomicBool>,
    found: Arc<AtomicBool>,
}

impl Terminator for BudgetTerminator {
    fn kill(&self) -> bool {
        self.interrupted.load(Ordering::Relaxed)
            || self.found.load(Ordering::Relaxed)
            || Instant::now() >= self.phase_deadline
    }

    fn new_timeout(&mut self, timeout: Duration) {
        self.phase_deadline = (Instant::now() + timeout).min(self.deadline);
    }

    fn timeout_at(&self) -> Option<Instant> {
        Some(self.phase_deadline)
    }
}

fn subset(input: &MultiBoardInput, counts: &[u64], height: f32) -> ExtSPInstance {
    ExtSPInstance {
        name: input.name.clone(),
        min_item_separation: input.min_item_separation,
        strip_height: height,
        items: input
            .items
            .iter()
            .zip(counts)
            .filter_map(|(item, &count)| {
                (count > 0).then(|| {
                    let mut item = item.clone();
                    item.demand = count;
                    item
                })
            })
            .collect(),
    }
}

fn available(deadline: Instant, interrupted: &AtomicBool) -> bool {
    Instant::now() < deadline && !interrupted.load(Ordering::Relaxed)
}

struct PlannedBoard {
    solution: SPSolution,
    counts: Vec<u64>,
    board_idx: usize,
    area: f64,
}

#[derive(Default)]
struct PlanScore {
    placed: Vec<u64>,
    item_area: f64,
    used: Vec<u64>,
    board_area: f64,
    weighted_item_area: f64,
    weighted_board_area: f64,
}

fn utilization_weights(boards: &[Board]) -> Vec<usize> {
    let mut priorities: Vec<_> = boards.iter().map(|board| board.priority).collect();
    priorities.sort_unstable();
    priorities.dedup();
    boards
        .iter()
        .map(|board| priorities.binary_search(&board.priority).unwrap() + 1)
        .collect()
}

impl PlanScore {
    fn overall_utilization(&self) -> f64 {
        if self.board_area > 0.0 {
            self.item_area / self.board_area
        } else {
            0.0
        }
    }

    fn weighted_utilization(&self) -> f64 {
        if self.weighted_board_area > 0.0 {
            self.weighted_item_area / self.weighted_board_area
        } else {
            0.0
        }
    }
    fn better_than(&self, other: &Self, board_order: &[usize]) -> bool {
        let count = self.placed.iter().sum::<u64>();
        let other_count = other.placed.iter().sum::<u64>();
        if count != other_count {
            return count > other_count;
        }
        if self.item_area != other.item_area {
            return self.item_area > other.item_area;
        }
        // Board preferences are comparable only for identical placed content.
        if self.placed != other.placed {
            return false;
        }
        let weighted = self.weighted_utilization();
        let other_weighted = other.weighted_utilization();
        if weighted != other_weighted {
            return weighted > other_weighted;
        }
        if self.board_area != other.board_area {
            return self.board_area < other.board_area;
        }
        let boards = self.used.iter().sum::<u64>();
        let other_boards = other.used.iter().sum::<u64>();
        if boards != other_boards {
            return boards < other_boards;
        }
        for &idx in board_order.iter().rev() {
            if self.used[idx] != other.used[idx] {
                return self.used[idx] < other.used[idx];
            }
        }
        false
    }
}

struct Plan {
    boards: Vec<PlannedBoard>,
    remaining: Vec<u64>,
    score: PlanScore,
}

impl Plan {
    fn empty(input: &MultiBoardInput) -> Self {
        Self {
            boards: Vec::new(),
            remaining: input.items.iter().map(|item| item.demand).collect(),
            score: PlanScore {
                placed: vec![0; input.items.len()],
                used: vec![0; input.boards.len()],
                ..PlanScore::default()
            },
        }
    }
}

#[cfg(feature = "live_svg")]
fn prefix_svg_ids(node: &mut dyn svg::Node, prefix: &str) {
    if let Some(attributes) = node.get_attributes_mut() {
        for (key, value) in attributes {
            let text = value.to_string();
            if key == "id" {
                *value = format!("{prefix}{text}").into();
            } else if matches!(key.as_str(), "href" | "xlink:href") && text.starts_with('#') {
                *value = format!("#{prefix}{}", &text[1..]).into();
            } else if text.contains("url(#") {
                *value = text.replace("url(#", &format!("url(#{prefix}")).into();
            }
        }
    }
    if let Some(children) = node.get_children_mut() {
        for child in children {
            prefix_svg_ids(child.as_mut(), prefix);
        }
    }
}

#[cfg(feature = "live_svg")]
fn build_live_svg(input: &MultiBoardInput, plan: &Plan, round: u64, status: &str) -> svg::Document {
    use svg::node::element::{Group, Rectangle, Text};

    fn label(text: String, y: f64, size: u32) -> Text {
        Text::new(text)
            .set("x", 24)
            .set("y", y)
            .set("font-size", size)
            .set("font-family", "sans-serif")
            .set("fill", "#172b4d")
    }

    let placed = plan.score.placed.iter().sum::<u64>();
    let demand = input.items.iter().map(|item| item.demand).sum::<u64>();
    let mut content = Group::new()
        .add(label(
            format!("Current global best | {}", input.name),
            32.0,
            22,
        ))
        .add(label(
            format!("Placed {placed}/{demand} | Best round {round} | Status: {status}"),
            62.0,
            18,
        ));
    content = content.add(label(
        format!(
            "Overall utilization: {:.2}% | Weighted utilization: {:.2}%",
            plan.score.overall_utilization() * 100.0,
            plan.score.weighted_utilization() * 100.0
        ),
        90.0,
        18,
    ));
    let weights = utilization_weights(&input.boards);
    let mut y = 122.0;
    for (idx, planned) in plan.boards.iter().enumerate() {
        let board = &input.boards[planned.board_idx];
        let number = idx as u64 + 1;
        let utilization = planned.area / (f64::from(board.length) * f64::from(board.width));
        content = content
            .add(label(
                format!("board_{number:06} | Board type: {}", board.id),
                y,
                20,
            ))
            .add(label(
                format!(
                    "Size: {} x {} | Priority: {} | Utilization: {:.2}%",
                    board.length,
                    board.width,
                    board.priority,
                    utilization * 100.0
                ),
                y + 26.0,
                18,
            ))
            .add(label(
                format!("Utilization weight: {}", weights[planned.board_idx]),
                y + 50.0,
                18,
            ));
        let height = 960.0 * f64::from(board.width) / f64::from(board.length);
        let mut panel = s_layout_to_svg(
            planned.solution.layout_snapshot(),
            number,
            DRAW_OPTIONS,
            &format!("board_{number:06}"),
        )
        .set("x", 24)
        .set("y", y + 64.0)
        .set("width", 960)
        .set("height", height)
        .set("preserveAspectRatio", "xMidYMid meet");
        prefix_svg_ids(&mut panel, &format!("live_board_{number}_"));
        content = content.add(panel);
        y += height + 104.0;
    }
    if plan.boards.is_empty() {
        content = content.add(label(
            "Waiting for a feasible board layout".to_owned(),
            y + 24.0,
            20,
        ));
        y += 80.0;
    }
    svg::Document::new()
        .set("viewBox", format!("0 0 1008 {}", y + 12.0))
        .set("width", 1008)
        .set("height", y + 12.0)
        .add(
            Rectangle::new()
                .set("width", "100%")
                .set("height", "100%")
                .set("fill", "white"),
        )
        .add(content)
}

#[cfg(feature = "live_svg")]
fn write_live_svg(
    input: &MultiBoardInput,
    options: &MultiBoardOptions,
    plan: &Plan,
    round: u64,
    status: &str,
) -> Result<()> {
    use anyhow::Context;
    if let Some(path) = &options.live_svg_path {
        svg::save(path, &build_live_svg(input, plan, round, status)).with_context(|| {
            format!("failed to write multiboard live SVG to {}", path.display())
        })?;
    }
    Ok(())
}

/// Reuses the unmodified strip optimizer and validates every accepted fixed-board layout.
pub fn solve(
    input: &MultiBoardInput,
    options: &MultiBoardOptions,
    output: &Path,
) -> Result<MultiBoardSummary> {
    input.validate()?;
    ensure!(options.workers != Some(0), "workers must be positive");
    let start = Instant::now();
    let deadline = start
        .checked_add(options.time_limit)
        .ok_or_else(|| anyhow::anyhow!("time limit is too large"))?;
    std::fs::create_dir_all(output)?;
    let mut best_plan = Plan::empty(input);
    #[cfg(feature = "live_svg")]
    write_live_svg(input, options, &best_plan, 0, "starting")?;
    let mut config = DEFAULT_SPARROW_CONFIG;
    if let Some(workers) = options.workers {
        config.expl_cfg.separator_config.n_workers = workers;
        config.cmpr_cfg.separator_config.n_workers = workers;
    }
    let importer = Importer::new(
        config.cde_config,
        config.poly_simpl_tolerance,
        config.narrow_concavity_cutoff_ratio,
    );
    // Import every shape, including zero-demand entries, to validate external geometry.
    let areas = input
        .items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            importer
                .import_item(&item.base, idx)
                .map(|item| f64::from(item.area()))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut item_order: Vec<_> = (0..input.items.len()).collect();
    item_order.sort_by(|&a, &b| areas[b].total_cmp(&areas[a]));
    let mut board_order: Vec<_> = (0..input.boards.len()).collect();
    board_order.sort_by(|&a, &b| input.boards[b].priority.cmp(&input.boards[a].priority));
    let weights = utilization_weights(&input.boards);
    let mut attempts = 0;
    let mut failures = 0;
    let mut search_rounds = 0;
    let mut best_round = 0;
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(options.seed);
    let needs_search = best_plan.remaining.iter().any(|&n| n > 0)
        && input.boards.iter().any(|board| board.quantity > 0);

    while needs_search && available(deadline, &options.interrupted) {
        search_rounds += 1;
        let mut round_board_order = board_order.clone();
        if search_rounds % 2 == 0 && round_board_order.len() > 1 {
            round_board_order.shuffle(&mut rng);
            if round_board_order == board_order {
                round_board_order.rotate_left(1);
            }
        }
        if search_rounds > 1 && item_order.len() > 1 {
            let previous_first = item_order[0];
            item_order.shuffle(&mut rng);
            if item_order[0] == previous_first {
                item_order.rotate_left(1);
            }
        }
        let remaining_time = deadline.saturating_duration_since(Instant::now());
        let half_remaining = remaining_time.div_f64(2.0);
        let round_budget = if search_rounds == 1 {
            half_remaining
        } else if search_rounds % 2 == 0 {
            half_remaining.min(Duration::from_secs(2))
        } else {
            let growth = (search_rounds / 2).min(10) as u32;
            half_remaining.min(Duration::from_secs(2 * (1u64 << growth)))
        };
        let round_deadline = (Instant::now() + round_budget).min(deadline);
        let mut plan = Plan::empty(input);
        while plan.remaining.iter().any(|&qty| qty > 0)
            && available(round_deadline, &options.interrupted)
        {
            let mut committed = false;
            for &bi in &round_board_order {
                let board = &input.boards[bi];
                if plan.score.used[bi] == board.quantity {
                    continue;
                }
                let mut counts = vec![0u64; plan.remaining.len()];
                let mut best: Option<SPSolution> = None;
                let mut area = 0.0;
                for &ii in &item_order {
                    for _ in 0..plan.remaining[ii] {
                        if !available(round_deadline, &options.interrupted) {
                            break;
                        }
                        if area + areas[ii] > f64::from(board.length) * f64::from(board.width) {
                            break;
                        }
                        counts[ii] += 1;
                        let ext = subset(input, &counts, board.width);
                        let mut accepted = None;
                        for _ in 0..2 {
                            if !available(round_deadline, &options.interrupted) {
                                break;
                            }
                            attempts += 1;
                            let candidate_deadline = (Instant::now()
                                + round_deadline
                                    .saturating_duration_since(Instant::now())
                                    .div_f64(
                                        plan.remaining.iter().sum::<u64>() as f64
                                            * board_order.len() as f64
                                            * 2.0
                                            + 1.0,
                                    ))
                            .min(round_deadline);
                            match candidate(
                                &ext,
                                board,
                                &importer,
                                config,
                                &mut rng,
                                candidate_deadline,
                                &options.interrupted,
                            ) {
                                Ok(Some(solution)) => {
                                    accepted = Some(solution);
                                    break;
                                }
                                Ok(None) | Err(_) => failures += 1,
                            }
                        }
                        if let Some(solution) = accepted {
                            area += areas[ii];
                            best = Some(solution);
                        } else {
                            counts[ii] -= 1;
                            break;
                        }
                    }
                }
                if let Some(solution) = best {
                    for (idx, &count) in counts.iter().enumerate() {
                        plan.remaining[idx] -= count;
                        plan.score.placed[idx] += count;
                    }
                    plan.score.used[bi] += 1;
                    plan.score.item_area += area;
                    plan.score.board_area += f64::from(board.length) * f64::from(board.width);
                    plan.boards.push(PlannedBoard {
                        solution,
                        counts,
                        board_idx: bi,
                        area,
                    });
                    committed = true;
                    break;
                }
                if !available(round_deadline, &options.interrupted) {
                    break;
                }
            }
            if !committed {
                break;
            }
        }
        plan.score.item_area = plan
            .score
            .placed
            .iter()
            .zip(&areas)
            .map(|(&count, &area)| count as f64 * area)
            .sum();
        plan.score.board_area = 0.0;
        plan.score.weighted_item_area = 0.0;
        plan.score.weighted_board_area = 0.0;
        for planned in &mut plan.boards {
            planned.area = planned
                .counts
                .iter()
                .zip(&areas)
                .map(|(&count, &area)| count as f64 * area)
                .sum();
            let board = &input.boards[planned.board_idx];
            let board_area = f64::from(board.length) * f64::from(board.width);
            let weight = weights[planned.board_idx] as f64;
            plan.score.board_area += board_area;
            plan.score.weighted_item_area += weight * planned.area;
            plan.score.weighted_board_area += weight * board_area;
        }
        if plan.score.better_than(&best_plan.score, &board_order) {
            best_plan = plan;
            best_round = search_rounds;
            #[cfg(feature = "live_svg")]
            write_live_svg(input, options, &best_plan, best_round, "searching")?;
        }
    }
    let stop_reason = if !needs_search && best_plan.remaining.iter().all(|&n| n == 0) {
        StopReason::AllItemsPlaced
    } else if !needs_search {
        StopReason::InventoryExhausted
    } else if options.interrupted.load(Ordering::Relaxed) {
        StopReason::Interrupted
    } else {
        StopReason::TimeBudgetExhausted
    };
    #[cfg(feature = "live_svg")]
    {
        let reason = match stop_reason {
            StopReason::AllItemsPlaced => "final: all_items_placed",
            StopReason::InventoryExhausted => "final: inventory_exhausted",
            StopReason::Interrupted => "final: interrupted",
            StopReason::TimeBudgetExhausted => "final: time_budget_exhausted",
            StopReason::NoPlacementFound => "final: no_placement_found",
        };
        write_live_svg(input, options, &best_plan, best_round, reason)?;
    }
    let mut boards = Vec::new();
    for planned in best_plan.boards {
        let board = &input.boards[planned.board_idx];
        let number = boards.len() as u64 + 1;
        let instance_id = format!("board_{number:06}");
        let svg_name = format!("{instance_id}.svg");
        let document = s_layout_to_svg(
            planned.solution.layout_snapshot(),
            number,
            DRAW_OPTIONS,
            &instance_id,
        );
        svg::save(output.join(&svg_name), &document)?;
        let mut layout = export_layout_snapshot(planned.solution.layout_snapshot(), number);
        let utilization = planned.area / (f64::from(board.length) * f64::from(board.width));
        layout.density = utilization as f32;
        boards.push(BoardResult {
            instance_id,
            board_id: board.id.clone(),
            priority: board.priority,
            utilization_weight: weights[planned.board_idx],
            length: board.length,
            width: board.width,
            used_item_area: planned.area,
            utilization,
            items: quantities(input, &planned.counts),
            layout,
            svg: svg_name,
        });
    }
    let summary = MultiBoardSummary {
        name: input.name.clone(),
        min_item_separation: input.min_item_separation,
        seed: options.seed,
        elapsed_seconds: start.elapsed().as_secs_f64(),
        stop_reason,
        search_rounds,
        best_round,
        all_items_placed: best_plan.remaining.iter().all(|&n| n == 0),
        candidate_attempts: attempts,
        failed_candidate_attempts: failures,
        overall_utilization: best_plan.score.overall_utilization(),
        weighted_utilization: best_plan.score.weighted_utilization(),
        boards,
        inventory: input
            .boards
            .iter()
            .zip(best_plan.score.used)
            .enumerate()
            .map(|(idx, (b, n))| BoardUsage {
                board_id: b.id.clone(),
                utilization_weight: weights[idx],
                available: b.quantity,
                used: n,
                remaining: b.quantity - n,
            })
            .collect(),
        unplaced: quantities(input, &best_plan.remaining),
    };
    serde_json::to_writer_pretty(
        std::fs::File::create(output.join("summary.json"))?,
        &summary,
    )?;
    Ok(summary)
}

fn quantities(input: &MultiBoardInput, counts: &[u64]) -> Vec<ItemQuantity> {
    input
        .items
        .iter()
        .zip(counts)
        .filter_map(|(item, &quantity)| {
            (quantity > 0).then_some(ItemQuantity {
                item_id: item.base.id,
                quantity,
            })
        })
        .collect()
}

fn candidate(
    ext: &ExtSPInstance,
    board: &Board,
    importer: &Importer,
    mut config: SparrowConfig,
    rng: &mut Xoshiro256PlusPlus,
    deadline: Instant,
    interrupted: &Arc<AtomicBool>,
) -> Result<Option<SPSolution>> {
    let instance = import_instance(importer, ext)?;
    let slice = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(1));
    config.expl_cfg.time_limit = slice.mul_f64(0.8);
    config.cmpr_cfg.time_limit = slice.mul_f64(0.2);
    let found = Arc::new(AtomicBool::new(false));
    let mut listener = FitListener {
        length: board.length,
        found: found.clone(),
    };
    let mut terminator = BudgetTerminator {
        deadline,
        phase_deadline: deadline,
        interrupted: interrupted.clone(),
        found,
    };
    let solution = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        optimize(
            instance.clone(),
            Xoshiro256PlusPlus::seed_from_u64(rng.next_u64()),
            &mut listener,
            &mut terminator,
            &config.expl_cfg,
            &config.cmpr_cfg,
            None,
        )
    })) {
        Ok(result) => result?,
        Err(_) => return Ok(None),
    };
    if !solution.strip_width().is_finite() || solution.strip_width() > board.length {
        return Ok(None);
    }
    let mut problem = SPProblem::new(instance)?;
    problem.restore(&solution);
    problem.change_strip_width(board.length)?;
    if problem.item_demand_qtys().iter().any(|&n| n != 0) || !problem.layout().is_collision_free() {
        return Ok(None);
    }
    Ok(Some(problem.save()))
}

#[cfg(test)]
mod tests {
    use super::PlanScore;

    #[cfg(feature = "live_svg")]
    #[test]
    fn initial_live_snapshot_and_reference_prefixing() {
        use super::*;
        use svg::node::element::{Definitions, Group, Use};
        let input: MultiBoardInput = serde_json::from_value(serde_json::json!({
            "name": "Job <&> \"quoted\"", "boards": [], "items": []
        }))
        .unwrap();
        let document = build_live_svg(&input, &Plan::empty(&input), 0, "starting").to_string();
        assert!(document.contains("Job &lt;&amp;&gt; \"quoted\""));
        assert!(document.contains("Placed 0/0 | Best round 0 | Status: starting"));
        assert!(document.contains("Overall utilization: 0.00% | Weighted utilization: 0.00%"));
        assert!(!document.contains("NaN"));
        assert!(document.contains("Waiting for a feasible board layout"));
        let path =
            std::env::temp_dir().join(format!("sparrow-live-initial-{}.svg", std::process::id()));
        std::fs::write(&path, "stale snapshot").unwrap();
        let options = MultiBoardOptions {
            live_svg_path: Some(path.clone()),
            ..MultiBoardOptions::default()
        };
        write_live_svg(&input, &options, &Plan::empty(&input), 0, "starting").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), document);
        std::fs::remove_file(path).unwrap();
        let mut group = Group::new()
            .add(Definitions::new().add(Group::new().set("id", "shape")))
            .add(
                Use::new()
                    .set("href", "#shape")
                    .set("xlink:href", "#shape")
                    .set("clip-path", "url(#shape)"),
            );
        prefix_svg_ids(&mut group, "board_1_");
        let text = group.to_string();
        assert!(text.contains("id=\"board_1_shape\""));
        assert!(text.contains("href=\"#board_1_shape\""));
        assert!(text.contains("xlink:href=\"#board_1_shape\""));
        assert!(text.contains("url(#board_1_shape)"));
    }

    fn score(placed: &[u64], item_area: f64, used: &[u64], board_area: f64) -> PlanScore {
        PlanScore {
            placed: placed.to_vec(),
            item_area,
            used: used.to_vec(),
            board_area,
            weighted_item_area: item_area,
            weighted_board_area: board_area,
        }
    }

    #[test]
    fn score_preserves_content_and_never_replaces_with_worse() {
        let best = score(&[2, 0], 10.0, &[1, 0], 20.0);
        assert!(!score(&[1, 0], 100.0, &[0, 0], 0.0).better_than(&best, &[0, 1]));
        assert!(score(&[1, 1], 11.0, &[0, 2], 40.0).better_than(&best, &[0, 1]));
        assert!(!score(&[1, 1], 10.0, &[0, 0], 0.0).better_than(&best, &[0, 1]));
        assert!(!best.better_than(&best, &[0, 1]));
    }

    #[test]
    fn score_uses_weighted_utilization_before_priority_and_ties() {
        let high_only = score(&[3], 27.0, &[3, 0, 0], 60.0);
        let fewer_low = score(&[3], 27.0, &[0, 0, 1], 30.0);
        assert!(fewer_low.better_than(&high_only, &[0, 1, 2]));
        assert!(!high_only.better_than(&fewer_low, &[0, 1, 2]));
        let mut weighted = score(&[3], 27.0, &[2, 0, 0], 60.0);
        weighted.weighted_item_area = 50.0;
        weighted.weighted_board_area = 55.0;
        assert!(weighted.better_than(&fewer_low, &[0, 1, 2]));
        let partial = score(&[2], 26.0, &[0, 0, 1], 26.0);
        assert!(!partial.better_than(&high_only, &[0, 1, 2]));
        let equal = score(&[3], 27.0, &[0, 3, 0], 60.0);
        assert!(high_only.better_than(&equal, &[0, 1, 2]));
        let fewer = score(&[3], 27.0, &[0, 0, 1], 60.0);
        assert!(fewer.better_than(&high_only, &[0, 1, 2]));
        let mut compact = score(&[3], 27.0, &[0, 0, 3], 50.0);
        compact.weighted_item_area = 27.0;
        compact.weighted_board_area = 60.0;
        assert!(compact.better_than(&high_only, &[0, 1, 2]));
    }

    #[test]
    fn weights_rank_all_priorities_without_signed_arithmetic() {
        use super::*;
        let boards: Vec<_> = [i64::MAX, 0, i64::MIN, -1, i64::MAX]
            .into_iter()
            .map(|priority| Board {
                id: priority.to_string(),
                length: 1.0,
                width: 1.0,
                quantity: 0,
                priority,
            })
            .collect();
        assert_eq!(utilization_weights(&boards), [4, 3, 1, 2, 4]);
        assert!(utilization_weights(&[]).is_empty());
    }

    #[test]
    fn utilization_is_ratio_of_weighted_areas_not_mean_of_ratios() {
        let mut baseline = score(&[1], 60.0, &[1, 1], 120.0);
        baseline.weighted_item_area = 2.0 * 10.0 + 50.0;
        baseline.weighted_board_area = 2.0 * 20.0 + 100.0;
        assert_eq!(baseline.weighted_utilization(), 0.5);
        let mut improved = score(&[1], 60.0, &[1, 1], 120.0);
        improved.weighted_item_area = 2.0 * 20.0 + 40.0;
        improved.weighted_board_area = 140.0;
        assert!((improved.weighted_utilization() - 80.0 / 140.0).abs() < 1e-12);
        assert_ne!(improved.weighted_utilization(), (1.0 + 0.4) / 2.0);
        assert!(improved.better_than(&baseline, &[0, 1]));
        assert_eq!(PlanScore::default().overall_utilization(), 0.0);
        assert_eq!(PlanScore::default().weighted_utilization(), 0.0);
    }
}
