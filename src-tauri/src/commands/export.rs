use crate::core::{exporter, models::*};

#[tauri::command]
pub async fn export_result(
    result: GroupingResult,
    dataset: Dataset,
    selected_indicators: Vec<String>,
    output_path: String,
    group_constraints: Option<Vec<SexConstraint>>,
) -> Result<(), String> {
    let sheet_config = exporter::SheetConfig {
        selected_indicators,
        include_statistics: true,
        include_summary: true,
        group_constraints,
    };

    exporter::export_grouping_result(&result, &dataset, &sheet_config, &output_path)
        .map_err(|e| format!("Export failed: {e}"))
}

#[tauri::command]
pub async fn export_multiple_results(
    multi_result: MultiGroupingResult,
    dataset: Dataset,
    selected_indicators: Vec<String>,
    output_path: String,
    group_constraints: Option<Vec<SexConstraint>>,
) -> Result<(), String> {
    let sheet_config = exporter::SheetConfig {
        selected_indicators,
        include_statistics: true,
        include_summary: true,
        group_constraints,
    };

    exporter::export_multiple_results(&multi_result, &dataset, &sheet_config, &output_path)
        .map_err(|e| format!("Export failed: {e}"))
}
