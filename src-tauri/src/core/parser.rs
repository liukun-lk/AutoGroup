use super::models::*;
use anyhow::{anyhow, Context, Result};
use calamine::{open_workbook, Data, DataType, Reader, Xlsx};
use std::collections::HashMap;

pub fn parse_excel_file(path: &str) -> Result<Dataset> {
    let mut workbook: Xlsx<_> =
        open_workbook(path).with_context(|| format!("Failed to open Excel file: {path}"))?;

    // Get first sheet (原始数据)
    let sheet_names = workbook.sheet_names().to_vec();
    if sheet_names.is_empty() {
        return Err(anyhow!("Excel file has no sheets"));
    }

    let first_sheet_name = sheet_names[0].clone();
    let range = workbook
        .worksheet_range(&first_sheet_name)
        .with_context(|| format!("Failed to read sheet: {first_sheet_name}"))?;

    // Collect all rows
    let mut rows = Vec::new();
    for row in range.rows() {
        rows.push(row.to_vec());
    }

    if rows.len() < 3 {
        return Err(anyhow!(
            "Excel file must have at least 3 rows (header + labels + data)"
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
        let sex = match row[1].get_string() {
            Some(s) => Sex::from_str(s)
                .map_err(|e| anyhow!("Invalid sex at row {}: {}", row_idx + 1, e))?,
            None => {
                return Err(anyhow!(
                    "Missing sex at row {} for animal {}",
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
        return Err(anyhow!("No valid animals found in Excel file"));
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
    !s.is_empty()
        && (s.contains('/')
            || s.contains('(')
            || s.contains("mol")
            || s.contains('^')
            || s == "kg"
            || s == "℃"
            || s == "U"
            || s == "g"
            || s == "L")
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
        return Err(anyhow!("No data rows found after header"));
    }

    Ok(data_row)
}

/// Detect which row contains the column headers
///
/// Header row typically contains keywords like "动物编号", "性别", "AnimalID", "Sex"
///
/// Returns the row index of the header
fn detect_header_row(rows: &[Vec<Data>]) -> Result<usize> {
    if rows.len() < 2 {
        return Err(anyhow!("Excel file must have at least 2 rows"));
    }

    // Look for header row by detecting common header keywords
    let header_keywords = ["动物编号", "性别", "AnimalID", "Sex", "Animal", "ID"];

    for (row_idx, row) in rows.iter().enumerate() {
        if row.is_empty() {
            continue;
        }

        // Check first two columns for header keywords
        let has_header_pattern = row.iter().take(2).any(|cell| {
            if let Some(s) = cell.get_string() {
                let lower = s.to_lowercase();
                header_keywords
                    .iter()
                    .any(|kw| lower.contains(&kw.to_lowercase()))
            } else {
                false
            }
        });

        if has_header_pattern {
            return Ok(row_idx);
        }
    }

    // Fallback: If no header detected, assume row 1 is header
    if rows.len() > 1 {
        Ok(1)
    } else {
        Err(anyhow!("Cannot determine header row"))
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
    if !prev_is_unit && !prev_is_simple && curr_is_unit {
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
