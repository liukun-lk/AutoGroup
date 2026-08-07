use super::models::*;
use anyhow::{anyhow, Result};
use calamine::{open_workbook, Data, DataType, Reader, Xlsx};
use std::collections::HashMap;

pub fn parse_excel_file(path: &str) -> Result<Dataset> {
    let file_name = file_name_of(path);

    // calamine's Xlsx reader only understands the OOXML format; a legacy .xls
    // would otherwise fail with an opaque zip error.
    ensure_xlsx_extension(path, &file_name)?;

    let mut workbook: Xlsx<_> = open_workbook(path).map_err(|e| {
        anyhow!(
            "无法打开文件「{file_name}」：{e}。\n\
             请确认：① 文件确实是 Excel 的 .xlsx 格式（而非改了后缀的 .xls / .csv）；\
             ② 文件没有损坏；③ 文件当前没有被 Excel 或 WPS 打开占用。"
        )
    })?;

    // Get first sheet (原始数据)
    let sheet_names = workbook.sheet_names().to_vec();
    if sheet_names.is_empty() {
        return Err(anyhow!(
            "文件「{file_name}」中没有任何工作表。请确认上传的是包含「原始数据」工作表的实验数据文件。"
        ));
    }

    let first_sheet_name = sheet_names[0].clone();
    let range = workbook.worksheet_range(&first_sheet_name).map_err(|e| {
        anyhow!("无法读取工作表「{first_sheet_name}」：{e}。请尝试在 Excel 中重新另存为 .xlsx 后再上传。")
    })?;

    // Collect all rows
    let mut rows = Vec::new();
    for row in range.rows() {
        rows.push(row.to_vec());
    }

    if rows.len() < 3 {
        return Err(anyhow!(
            "工作表「{}」只有 {} 行内容，不足以解析。\n\
             文件至少需要 3 行：第 1 行英文指标名或单位、第 2 行中文列名、第 3 行起为动物数据。",
            first_sheet_name,
            rows.len()
        ));
    }

    // Detect which row contains the actual header
    // Header row typically has keywords like "动物编号", "性别", "AnimalID", "Sex"
    let header_row_idx = detect_header_row(&rows)?;

    // Parse header rows for indicator names
    // Support both single-row and dual-row headers
    // - Dual-row header: Row N-1 has short names/units, Row N has Chinese names/units
    // - Single-row header: Only Row N has indicator names, Row N-1 may have units
    let prev_row = if header_row_idx > 0 {
        Some(&rows[header_row_idx - 1])
    } else {
        None
    };
    let header_row = &rows[header_row_idx];

    let mut indicator_names: Vec<String> = Vec::new();
    let mut indicator_metadata: Vec<IndicatorMetadata> = Vec::new();

    let max_cols = header_row.len();

    // Start from column 2 (skip AnimalID and Sex columns)
    for col_idx in 2..max_cols {
        let header_val = header_row
            .get(col_idx)
            .and_then(|c| c.get_string())
            .unwrap_or("");
        let prev_val = prev_row
            .and_then(|row| row.get(col_idx))
            .and_then(|c| c.get_string())
            .unwrap_or("");

        let header_trimmed = header_val.trim();
        let prev_trimmed = prev_val.trim();

        // Skip if header is empty
        if header_trimmed.is_empty() {
            continue;
        }

        // Determine key, display_name, and unit from dual-row header
        let (key, display_name, unit) = parse_dual_row_header(prev_trimmed, header_trimmed);

        indicator_names.push(key.clone());
        indicator_metadata.push(IndicatorMetadata::new(key, display_name, unit));
    }

    if indicator_names.is_empty() {
        return Err(anyhow!(
            "在工作表「{}」的第 {} 行没有识别到任何指标列。\n\
             请确认第 1 列为动物编号、第 2 列为性别，第 3 列及之后为指标名称（如 体重、ALT、WBC）。",
            first_sheet_name,
            header_row_idx + 1
        ));
    }

    // Determine data start row by detecting header row pattern
    // Header row typically contains text like "动物编号", "性别", "AnimalID", "Sex"
    // Data row contains numeric/specific values
    let start_row = detect_data_start_row(&rows)?;

    let mut animals = Vec::new();

    for (row_idx, row) in rows.iter().enumerate().skip(start_row) {
        if row.is_empty() {
            continue;
        }

        // Column 0: AnimalID (support both string and numeric formats)
        let animal_id = match &row[0] {
            Data::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    continue;
                }
                trimmed.to_string()
            }
            Data::Int(i) => i.to_string(),
            Data::Float(f) => {
                // Format as integer if it's a whole number, otherwise with decimals
                if f.fract() == 0.0 {
                    format!("{f:.0}")
                } else {
                    f.to_string()
                }
            }
            Data::Empty => continue,
            _ => continue,
        };

        // Column 1: Sex
        let sex = match row.get(1).and_then(|cell| cell.get_string()) {
            Some(s) => Sex::from_str(s).map_err(|_| {
                anyhow!(
                    "第 {} 行（动物编号 {}）的性别「{}」无法识别。\n\
                     性别列（第 2 列）只接受：F / M、Female / Male、雌性 / 雄性。",
                    row_idx + 1,
                    animal_id,
                    s.trim()
                )
            })?,
            None => {
                return Err(anyhow!(
                    "第 {} 行（动物编号 {}）缺少性别。\n\
                     请在第 2 列填写 F / M 或 雌性 / 雄性；若该行是空行或备注行，请从数据区删除。",
                    row_idx + 1,
                    animal_id
                ))
            }
        };

        // Columns 2+: Indicators
        let mut indicators = HashMap::new();
        for (col_idx, cell) in row.iter().enumerate().skip(2) {
            let indicator_idx = col_idx - 2;
            if indicator_idx >= indicator_names.len() {
                break;
            }

            if let Some(value) = cell.get_float() {
                indicators.insert(indicator_names[indicator_idx].clone(), value);
            }
        }

        animals.push(Animal {
            id: animal_id,
            sex,
            indicators,
        });
    }

    if animals.is_empty() {
        return Err(anyhow!(
            "在工作表「{}」的第 {} 行及之后没有解析到任何动物数据。\n\
             请确认数据行紧跟在表头行下方，且第 1 列填写了动物编号。",
            first_sheet_name,
            start_row + 1
        ));
    }

    let male_count = animals.iter().filter(|a| a.sex == Sex::Male).count();
    let female_count = animals.iter().filter(|a| a.sex == Sex::Female).count();

    let metadata = DatasetMetadata {
        total_animals: animals.len(),
        male_count,
        female_count,
        indicator_count: indicator_names.len(),
    };

    Ok(Dataset {
        indicator_names,
        indicator_metadata,
        metadata,
        animals,
    })
}

/// Check if string is a unit (contains /, parentheses, or common unit patterns)
fn is_unit_string(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Pure unit patterns (not indicator names with units)
    // Units are typically short and contain only unit characters
    let common_pure_units = [
        "kg", "g", "mg", "ug", "ng", "℃", "°C", "L", "mL", "uL", "U", "IU", "mol", "mmol", "umol",
        "nmol", "m", "cm", "mm", "sec", "min", "h", "%", "fL", "pg", "U/L", "g/L", "mg/L",
        "mmol/L", "umol/L", "nmol/L", "10^9/L", "10^12/L", "deg", "A/G",
        "AST/ALT", // Ratio indicators treated as units in some cases
    ];

    // Check if it's exactly a common pure unit
    if common_pure_units.contains(&s) {
        return true;
    }

    // For strings with parentheses or slashes, check if they look like pure units
    // Pure units: "U/L", "10^9/L", "mmol/L" - short, no uppercase letters at start
    // Not units: "WBC(10^9/L)", "RBC(10^12/L)" - have prefix before parenthesis

    // If contains '(', check if it has indicator prefix before parenthesis
    if let Some(paren_idx) = s.find('(') {
        // If there's text before '(', it's likely an indicator name with unit
        // e.g., "WBC(10^9/L)" has "WBC" before '('
        if paren_idx > 0 && s[..paren_idx].chars().any(|c| c.is_ascii_uppercase()) {
            return false;
        }
    }

    // Short strings with only unit-like characters
    s.len() <= 10 && (s.contains('/') || s.contains('^') || s.contains("mol"))
}

// The following functions are currently unused but kept for potential future use

/// Check if a string looks like a unit (contains special chars or is very short)
#[allow(dead_code)]
fn is_unit_like(s: &str) -> bool {
    // Units typically contain: parentheses, /, ^, or are very short (kg, ℃)
    s.len() <= 3 || s.contains('(') || s.contains('/') || s.contains('^') || s.contains('℃')
}

/// Check if a string is a Chinese name or indicator abbreviation
#[allow(dead_code)]
fn is_chinese_or_indicator_name(s: &str) -> bool {
    // Contains Chinese characters or looks like indicator name (WBC, RBC, etc.)
    let has_chinese = s.chars().any(|c| {
        let code = c as u32;
        code > 0x4E00 && code < 0x9FFF
    });

    let is_indicator = s.len() > 2
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c == '#' || c == '%' || c == '-');

    has_chinese || is_indicator
}

/// Extract unit from text (handles patterns like "U/L", "g/L", "体重")
#[allow(dead_code)]
fn extract_unit_from_text(text: &str) -> String {
    // If text contains common unit patterns, extract them
    if text.contains("U/L") {
        "U/L".to_string()
    } else if text.contains("g/L") {
        "g/L".to_string()
    } else if text.contains("mmol/L") {
        "mmol/L".to_string()
    } else if text.contains("umol/L") {
        "umol/L".to_string()
    } else if text.contains("10^9/L") {
        "(10^9/L)".to_string()
    } else if text.contains("10^12/L") {
        "(10^12/L)".to_string()
    } else {
        // No recognizable unit
        String::new()
    }
}

/// Detect the row where actual data starts (after headers)
///
/// Strategy: Find the row containing column headers (e.g., "动物编号", "AnimalID", "Sex")
/// Data starts immediately after this header row
///
/// Returns the row index where data begins
fn detect_data_start_row(rows: &[Vec<Data>]) -> Result<usize> {
    let header_row_idx = detect_header_row(rows)?;
    let data_row = header_row_idx + 1;

    if data_row >= rows.len() {
        return Err(anyhow!(
            "表头行（第 {} 行）之后没有任何数据行。请在表头下方填入动物数据后重新上传。",
            header_row_idx + 1
        ));
    }

    Ok(data_row)
}

/// Detect which row contains the column headers
///
/// The primary rule is data-driven: a data row is recognizable by its second column
/// holding a sex literal, so the header is the row directly above the first such row.
/// Column-name keywords are only a fallback — real files name the first column
/// `Serial#` and leave the sex column header empty, neither of which any keyword list
/// can be relied upon to cover.
///
/// Returns the row index of the header
fn detect_header_row(rows: &[Vec<Data>]) -> Result<usize> {
    if rows.len() < 2 {
        return Err(anyhow!(
            "工作表内容不足 2 行，无法定位表头。请确认上传的是完整的实验数据文件。"
        ));
    }

    if let Some(first_data_row) = rows.iter().position(|row| is_data_row(row)) {
        if first_data_row >= 1 {
            return Ok(first_data_row - 1);
        }
    }

    // Fallback: keyword match. The *last* matching row wins — with a dual-row header
    // whose first row carries `AnimalID` / `Sex`, taking the first match would treat the
    // Chinese header row as data.
    let header_keywords = ["动物编号", "性别", "AnimalID", "Sex", "Animal", "ID"];

    let keyword_match = rows.iter().rposition(|row| {
        row.iter().take(2).any(|cell| {
            cell.get_string().is_some_and(|s| {
                let lower = s.to_lowercase();
                header_keywords
                    .iter()
                    .any(|kw| lower.contains(&kw.to_lowercase()))
            })
        })
    });

    // A match on the very last row cannot be a header — nothing would follow it.
    match keyword_match {
        Some(idx) if idx + 1 < rows.len() => Ok(idx),
        // Fallback: If no header detected, assume row 1 is header
        _ => Ok(1),
    }
}

/// A data row carries an animal id in column 1 and a recognizable sex in column 2.
fn is_data_row(row: &[Data]) -> bool {
    let has_id = match row.first() {
        Some(Data::String(s)) => !s.trim().is_empty(),
        Some(Data::Int(_)) | Some(Data::Float(_)) => true,
        _ => false,
    };

    has_id
        && row
            .get(1)
            .and_then(|cell| cell.get_string())
            .is_some_and(|s| Sex::from_str(s).is_ok())
}

/// Extract the file name for user-facing messages, falling back to the full path.
fn file_name_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn ensure_xlsx_extension(path: &str, file_name: &str) -> Result<()> {
    let ext = std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "xlsx" | "xlsm" => Ok(()),
        "xls" => Err(anyhow!(
            "文件「{file_name}」是旧版 .xls 格式，暂不支持。\n\
             请在 Excel / WPS 中打开该文件，选择「另存为」→ 格式选择「Excel 工作簿 (.xlsx)」后重新上传。"
        )),
        "" => Err(anyhow!(
            "文件「{file_name}」没有扩展名，无法确认格式。请上传 .xlsx 格式的 Excel 文件。"
        )),
        other => Err(anyhow!(
            "文件「{file_name}」的格式为 .{other}，暂不支持。请上传 .xlsx 格式的 Excel 文件（CSV 请先用 Excel 另存为 .xlsx）。"
        )),
    }
}

/// Parse indicator metadata from dual-row header
///
/// The original data format has a complex dual-row header structure:
/// - Row N-1: May contain English names, units (kg, ℃), or indicator abbreviations (ALT, AST)
/// - Row N: May contain Chinese names, units (U/L, g/L), or empty
///
/// Returns: (key, display_name, unit)
///
/// Priority for key selection:
/// 1. If Row N-1 has meaningful name (not just unit), use it
/// 2. Otherwise use Row N
///
/// Priority for display_name:
/// 1. Prefer Chinese name if available
/// 2. Otherwise use English name or abbreviation
///
/// Unit extraction:
/// - Detect common unit patterns (/, parentheses, mol, kg, ℃, etc.)
fn parse_dual_row_header(prev_row_val: &str, curr_row_val: &str) -> (String, String, String) {
    // Both empty shouldn't happen due to caller's check on curr_row_val
    if prev_row_val.is_empty() && curr_row_val.is_empty() {
        return ("Unknown".to_string(), "Unknown".to_string(), String::new());
    }

    let prev_is_unit = is_unit_string(prev_row_val);
    let prev_is_simple = is_simple_name(prev_row_val);
    let _prev_is_chinese = has_chinese_chars(prev_row_val);

    let curr_is_unit = is_unit_string(curr_row_val);
    let curr_is_chinese = has_chinese_chars(curr_row_val);

    // Case 1: prev_row has simple name (kg, ℃), curr_row has Chinese name (体重, 肛温)
    // Result: key="kg", display="体重", unit="kg"
    if prev_is_simple && curr_is_chinese && !curr_is_unit {
        return (
            prev_row_val.to_string(),
            curr_row_val.to_string(),
            prev_row_val.to_string(),
        );
    }

    // Case 2: prev_row has indicator name (ALT, AST), curr_row has unit (U/L)
    // Result: key="ALT", display="ALT", unit="U/L"
    // IMPORTANT: Only apply this case if prev_row is NOT empty
    if !prev_row_val.is_empty() && !prev_is_unit && !prev_is_simple && curr_is_unit {
        return (
            prev_row_val.to_string(),
            prev_row_val.to_string(),
            curr_row_val.to_string(),
        );
    }

    // Case 3: prev_row is unit (U), curr_row has indicator name (ALT, AST)
    // Result: key="ALT", display="ALT", unit="U"
    if prev_is_unit && !curr_is_unit {
        return (
            curr_row_val.to_string(),
            curr_row_val.to_string(),
            prev_row_val.to_string(),
        );
    }

    // Case 4: Both rows have content, neither is clear unit
    // Prefer Chinese for display if available
    if curr_is_chinese {
        let unit = if prev_is_unit {
            prev_row_val.to_string()
        } else {
            String::new()
        };
        return (curr_row_val.to_string(), curr_row_val.to_string(), unit);
    }

    // Case 5: prev_row has content, use it as both key and display
    if !prev_row_val.is_empty() {
        let unit = if curr_is_unit {
            curr_row_val.to_string()
        } else if prev_is_unit {
            prev_row_val.to_string()
        } else {
            String::new()
        };
        return (prev_row_val.to_string(), prev_row_val.to_string(), unit);
    }

    // Fallback: use curr_row as key and display
    (
        curr_row_val.to_string(),
        curr_row_val.to_string(),
        String::new(),
    )
}

/// Check if string is a simple short name (kg, ℃, etc.)
fn is_simple_name(s: &str) -> bool {
    s.len() <= 3 && (s == "kg" || s == "℃" || s == "°C" || s == "cm" || s == "g")
}

/// Check if string contains Chinese characters
fn has_chinese_chars(s: &str) -> bool {
    s.chars().any(|c| {
        let code = c as u32;
        (0x4E00..=0x9FFF).contains(&code)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_xlsx_and_xlsm() {
        assert!(ensure_xlsx_extension("/data/raw.xlsx", "raw.xlsx").is_ok());
        assert!(ensure_xlsx_extension("/data/raw.XLSX", "raw.XLSX").is_ok());
        assert!(ensure_xlsx_extension("/data/raw.xlsm", "raw.xlsm").is_ok());
    }

    #[test]
    fn legacy_xls_is_rejected_with_conversion_hint() {
        let err = ensure_xlsx_extension("/data/raw.xls", "raw.xls").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("raw.xls"),
            "message should name the file: {msg}"
        );
        assert!(
            msg.contains("另存为"),
            "message should tell how to fix: {msg}"
        );
    }

    #[test]
    fn unsupported_and_missing_extensions_are_rejected() {
        let csv = ensure_xlsx_extension("/data/raw.csv", "raw.csv")
            .unwrap_err()
            .to_string();
        assert!(
            csv.contains(".csv"),
            "message should name the format: {csv}"
        );

        let none = ensure_xlsx_extension("/data/raw", "raw")
            .unwrap_err()
            .to_string();
        assert!(none.contains("扩展名"), "message should explain: {none}");
    }

    #[test]
    fn file_name_falls_back_to_full_path() {
        assert_eq!(file_name_of("/tmp/data/raw.xlsx"), "raw.xlsx");
        assert_eq!(file_name_of("raw.xlsx"), "raw.xlsx");
        assert_eq!(file_name_of("/"), "/");
    }

    fn text(s: &str) -> Data {
        Data::String(s.to_string())
    }

    /// Lab file shape: single header row, first column named `Serial#`, sex column
    /// header left empty. No keyword matches, so only the data-driven rule finds it.
    #[test]
    fn detects_header_above_first_data_row_without_keywords() {
        let rows = vec![
            vec![text("Serial#"), Data::Empty, text("BW(g)"), text("CD45%")],
            vec![
                Data::Int(1),
                text("F"),
                Data::Float(27.9),
                Data::Float(0.53),
            ],
            vec![
                Data::Int(2),
                text("F"),
                Data::Float(28.4),
                Data::Float(0.61),
            ],
        ];
        assert_eq!(detect_header_row(&rows).unwrap(), 0);
    }

    /// Dual-row header whose *first* row carries `AnimalID` / `Sex`: the keyword rule
    /// used to stop there and hand the Chinese header row to the data parser.
    #[test]
    fn dual_row_header_resolves_to_the_lower_row() {
        let rows = vec![
            vec![text("AnimalID"), text("Sex"), text("BW"), text("CD45")],
            vec![
                text("动物编号"),
                text("性别"),
                text("体重"),
                text("CD45 比例"),
            ],
            vec![
                Data::Int(1),
                text("雌性"),
                Data::Float(27.9),
                Data::Float(0.53),
            ],
        ];
        assert_eq!(detect_header_row(&rows).unwrap(), 1);
    }

    /// Project-standard shape: dual-row header with the first row left blank in the
    /// first two columns. Behaviour must be unchanged.
    #[test]
    fn blank_first_row_dual_header_is_unchanged() {
        let rows = vec![
            vec![Data::Empty, Data::Empty, text("BW"), text("CD45")],
            vec![
                text("动物编号"),
                text("性别"),
                text("体重"),
                text("CD45 比例"),
            ],
            vec![
                Data::Int(1),
                text("雌性"),
                Data::Float(27.9),
                Data::Float(0.53),
            ],
        ];
        assert_eq!(detect_header_row(&rows).unwrap(), 1);
    }

    #[test]
    fn parses_the_randomization_fixture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/randomization_input_60f.xlsx"
        );
        let dataset = parse_excel_file(path).unwrap();

        assert_eq!(dataset.metadata.total_animals, 60);
        assert_eq!(dataset.metadata.female_count, 60);
        assert_eq!(dataset.metadata.male_count, 0);
        assert_eq!(dataset.indicator_names, vec!["体重", "CD45 比例"]);
    }

    #[test]
    fn missing_file_reports_actionable_message() {
        let err = parse_excel_file("/definitely/missing/file.xlsx").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("file.xlsx"),
            "message should name the file: {msg}"
        );
        assert!(
            msg.contains("无法打开"),
            "message should be in Chinese: {msg}"
        );
    }
}
