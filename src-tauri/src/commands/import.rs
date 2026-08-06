use crate::core::{models::Dataset, parser, validator};

#[tauri::command]
pub async fn parse_excel(file_path: String) -> Result<Dataset, String> {
    // `{e:#}` keeps the whole anyhow chain so the user sees the concrete cause,
    // not just the outermost context line.
    let dataset =
        parser::parse_excel_file(&file_path).map_err(|e| format!("文件解析失败：{e:#}"))?;

    validator::validate_dataset(&dataset).map_err(|e| format!("数据校验未通过：{e:#}"))?;

    Ok(dataset)
}
