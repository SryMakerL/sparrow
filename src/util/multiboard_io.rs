use anyhow::{Context, Result, ensure};
use jagua_rs::io::ext_repr::ExtLayout;
use jagua_rs::probs::spp::io::ext_repr::ExtItem;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct MultiBoardInput {
    pub name: String,
    #[serde(default)]
    pub min_item_separation: f32,
    pub boards: Vec<Board>,
    pub items: Vec<ExtItem>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: String,
    pub length: f32,
    pub width: f32,
    pub quantity: u64,
    pub priority: i64,
}

impl MultiBoardInput {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.min_item_separation.is_finite() && self.min_item_separation >= 0.0,
            "min_item_separation must be finite and nonnegative"
        );
        let mut ids = HashSet::new();
        for board in &self.boards {
            ensure!(
                !board.id.trim().is_empty() && ids.insert(&board.id),
                "board IDs must be nonempty and unique"
            );
            ensure!(
                board.length.is_finite()
                    && board.length > 0.0
                    && board.width.is_finite()
                    && board.width > 0.0,
                "board {} dimensions must be finite and positive",
                board.id
            );
        }
        let mut ids = HashSet::new();
        let mut total = 0u64;
        for item in &self.items {
            ensure!(ids.insert(item.base.id), "item IDs must be unique");
            usize::try_from(item.demand).context("item demand exceeds platform capacity")?;
            total = total
                .checked_add(item.demand)
                .context("total item demand overflow")?;
        }
        usize::try_from(total).context("total demand exceeds platform capacity")?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct ItemQuantity {
    pub item_id: u64,
    pub quantity: u64,
}

#[derive(Serialize, Deserialize)]
pub struct BoardUsage {
    pub board_id: String,
    pub utilization_weight: usize,
    pub available: u64,
    pub used: u64,
    pub remaining: u64,
}

#[derive(Serialize, Deserialize)]
pub struct BoardResult {
    pub instance_id: String,
    pub board_id: String,
    pub priority: i64,
    pub utilization_weight: usize,
    pub length: f32,
    pub width: f32,
    pub used_item_area: f64,
    pub utilization: f64,
    pub items: Vec<ItemQuantity>,
    pub layout: ExtLayout,
    pub svg: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    AllItemsPlaced,
    TimeBudgetExhausted,
    Interrupted,
    InventoryExhausted,
    NoPlacementFound,
}

#[derive(Serialize, Deserialize)]
pub struct MultiBoardSummary {
    pub name: String,
    pub min_item_separation: f32,
    pub seed: u64,
    pub elapsed_seconds: f64,
    pub stop_reason: StopReason,
    pub search_rounds: u64,
    pub best_round: u64,
    pub all_items_placed: bool,
    pub candidate_attempts: u64,
    pub failed_candidate_attempts: u64,
    pub overall_utilization: f64,
    pub weighted_utilization: f64,
    pub boards: Vec<BoardResult>,
    pub inventory: Vec<BoardUsage>,
    pub unplaced: Vec<ItemQuantity>,
}

pub fn read_input(path: &Path) -> Result<MultiBoardInput> {
    let input: MultiBoardInput = serde_json::from_str(
        &std::fs::read_to_string(path).context("could not read multiboard input")?,
    )
    .context("could not parse multiboard input")?;
    input.validate()?;
    Ok(input)
}

pub fn safe_task_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .take(80)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        "multiboard".to_owned()
    } else {
        format!("job_{safe}")
    }
}

pub fn output_directory(root: &Path, name: &str) -> PathBuf {
    root.join(safe_task_name(name))
}
