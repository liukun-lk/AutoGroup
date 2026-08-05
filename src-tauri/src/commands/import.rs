use crate::core::{models::Dataset, parser, validator};

#[tauri::command]
pub async fn parse_excel(file_path: String) -> Result<Dataset, String> {
    let dataset =
        parser::parse_excel_file(&file_path).map_err(|e| format!("Failed to parse Excel: {e}"))?;

    validator::validate_dataset(&dataset).map_err(|e| format!("Data validation failed: {e}"))?;

    Ok(dataset)
}
