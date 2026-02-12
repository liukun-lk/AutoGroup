use super::models::*;
use anyhow::{anyhow, Context, Result};
use calamine::{open_workbook, DataType, Reader, Xlsx};
use std::collections::HashMap;

pub fn parse_excel_file(path: &str) -> Result<Dataset> {
    let mut workbook: Xlsx<_> = open_workbook(path)
        .with_context(|| format!("Failed to open Excel file: {}", path))?;

    // Get first sheet (原始数据)
    let sheet_names = workbook.sheet_names().to_vec();
    if sheet_names.is_empty() {
        return Err(anyhow!("Excel file has no sheets"));
    }

    let first_sheet_name = sheet_names[0].clone();
    let range = workbook
        .worksheet_range(&first_sheet_name)
        .with_context(|| format!("Failed to read sheet: {}", first_sheet_name))?;

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

    // Parse dual-row header
    // Row 0: English names (or units/empty)
    // Row 1: Chinese names + units
    let row0 = &rows[0];
    let row1 = &rows[1];

    let mut indicator_names: Vec<String> = Vec::new();
    let mut indicator_metadata: Vec<IndicatorMetadata> = Vec::new();

    // Start from column 2 (skip AnimalID and Sex columns)
    let max_cols = row0.len().max(row1.len());

    for col_idx in 2..max_cols {
        let row0_val = row0.get(col_idx).and_then(|c| c.get_string()).unwrap_or("");
        let row1_val = row1.get(col_idx).and_then(|c| c.get_string()).unwrap_or("");

        let row0_trimmed = row0_val.trim();
        let row1_trimmed = row1_val.trim();

        // Skip if both rows are empty
        if row0_trimmed.is_empty() && row1_trimmed.is_empty() {
            continue;
        }

        // Determine key, display_name, and unit
        let (key, display_name, unit) = parse_indicator_metadata(row0_trimmed, row1_trimmed);

        indicator_names.push(key.clone());
        indicator_metadata.push(IndicatorMetadata::new(key, display_name, unit));
    }

    // Determine data start row
    let start_row = if rows.len() > 2 {
        // Check if row 2 is data or additional header
        let first_cell = &rows[2][0];
        if let Some(s) = first_cell.get_string() {
            if s.starts_with("XHP") || s.len() > 5 {
                2 // Row 2 is data
            } else {
                3 // Row 2 is additional header, skip it
            }
        } else {
            3
        }
    } else {
        2
    };

    let mut animals = Vec::new();

    for (row_idx, row) in rows.iter().enumerate().skip(start_row) {
        if row.is_empty() {
            continue;
        }

        // Column 0: AnimalID
        let animal_id = match row[0].get_string() {
            Some(s) => s.trim().to_string(),
            None => continue,
        };

        if animal_id.is_empty() {
            continue;
        }

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

/// Parse indicator metadata from dual-row header
/// Returns: (key, display_name, unit)
///
/// The original data has a complex structure:
/// - Row 1 (Row 0 in code): Mix of English names/units (kg, ℃, ALT, AST, TP...)
/// - Row 2 (Row 1 in code): Mix of Chinese names/units (体重, 肛温, U/L, g/L...)
///
/// Pattern recognition:
/// - If Row 1 is a short name (kg, ℃) and Row 2 is Chinese -> Row 2 is display name
/// - If Row 1 is uppercase English (ALT, AST) and Row 2 is unit (U/L) -> Row 1 is display name, Row 2 is unit
fn parse_indicator_metadata(row0: &str, row1: &str) -> (String, String, String) {
    // Both empty shouldn't happen due to caller's check
    if row0.is_empty() && row1.is_empty() {
        return ("Unknown".to_string(), "Unknown".to_string(), String::new());
    }

    // Case 1: Row 0 is clearly a unit (kg, ℃), Row 1 should be Chinese name
    if is_simple_unit(row0) {
        let unit = row0.to_string();
        if !row1.is_empty() && is_chinese_name(row1) {
            // e.g., Row0="kg", Row1="体重" -> key="kg", display="体重", unit="kg"
            return (row0.to_string(), row1.to_string(), unit);
        } else {
            // e.g., Row0="kg", Row1=empty -> key="kg", display="kg", unit="kg"
            return (row0.to_string(), row0.to_string(), unit);
        }
    }

    // Case 2: Row 0 is English indicator name (ALT, AST, WBC...)
    if is_english_indicator_name(row0) {
        let key = row0.to_string();
        // Row 1 is likely a unit
        let unit = if is_unit_string(row1) {
            row1.to_string()
        } else {
            String::new()
        };
        // Use English name as display (no Chinese available)
        return (key.clone(), key, unit);
    }

    // Case 3: Row 0 is empty/unit, Row 1 has content
    if row0.is_empty() || is_unit_string(row0) {
        let unit = if is_unit_string(row0) {
            row0.to_string()
        } else if is_unit_string(row1) {
            row1.to_string()
        } else {
            String::new()
        };

        // Use Row 1 as both key and display
        return (row1.to_string(), row1.to_string(), unit);
    }

    // Fallback: Use Row 0 as key and display
    let unit = if is_unit_string(row1) {
        row1.to_string()
    } else {
        String::new()
    };
    (row0.to_string(), row0.to_string(), unit)
}

/// Check if string is a simple unit (kg, ℃, etc.)
fn is_simple_unit(s: &str) -> bool {
    matches!(s, "kg" | "℃" | "°C")
}

/// Check if string is a Chinese name (contains Chinese characters)
fn is_chinese_name(s: &str) -> bool {
    s.chars().any(|c| {
        let code = c as u32;
        code >= 0x4E00 && code <= 0x9FFF
    })
}

/// Check if string is an English indicator name (uppercase letters, possibly with numbers)
fn is_english_indicator_name(s: &str) -> bool {
    !s.is_empty() &&
    s.len() >= 2 &&
    s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '#' || c == '%')
}

/// Check if string is a unit (contains /, parentheses, or common unit patterns)
fn is_unit_string(s: &str) -> bool {
    !s.is_empty() && (
        s.contains('/') ||
        s.contains('(') ||
        s.contains("mol") ||
        s.contains("^") ||
        s == "kg" ||
        s == "℃"
    )
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

    let is_indicator = s.len() > 2 && s.chars().all(|c| c.is_ascii_uppercase() || c == '#' || c == '%' || c == '-');

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
