use crate::core::{grouping, models::*};

#[tauri::command]
pub async fn compute_grouping(
    dataset: Dataset,
    group_config: GroupConfig,
    stat_config: StatConfig,
) -> Result<MultiGroupingResult, String> {
    grouping::compute_grouping(dataset, group_config, stat_config)
        .map_err(|e| format!("Grouping computation failed: {e}"))
}
