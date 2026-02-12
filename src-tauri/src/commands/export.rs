use crate::core::{exporter, models::*};
use serde::Deserialize;

#[tauri::command]
pub async fn export_result(
    result: GroupingResult,
    dataset: Dataset,
    selected_indicators: Vec<String>,
    output_path: String,
) -> Result<(), String> {
    let export_config = exporter::ExportConfig {
        selected_indicators,
        include_statistics: true,
        include_summary: true,
    };

    exporter::export_grouping_result(&result, &dataset, &export_config, &output_path)
        .map_err(|e| format!("Export failed: {}", e))
}

#[derive(Debug, Deserialize)]
pub enum ExportFormat {
    Excel,
    Csv,
}
