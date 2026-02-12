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

    // Row 0: Indicator names (English, skip first 2 columns)
    let indicator_row = &rows[0];
    let mut indicator_names: Vec<String> = Vec::new();

    for (col_idx, cell) in indicator_row.iter().enumerate().skip(2) {
        let cell_str = cell.get_string().unwrap_or("");
        if !cell_str.trim().is_empty() {
            indicator_names.push(cell_str.trim().to_string());
        } else {
            // Try to get Chinese name from row 1
            if let Some(row1_cell) = rows.get(1).and_then(|r| r.get(col_idx)) {
                if let Some(s) = row1_cell.get_string() {
                    if !s.trim().is_empty() {
                        indicator_names.push(s.trim().to_string());
                        continue;
                    }
                }
            }
            // Generate placeholder name
            indicator_names.push(format!("Indicator_{}", col_idx - 1));
        }
    }

    // Determine data start row
    let start_row = if rows.len() > 2 {
        // Check if row 2 is data or unit row
        let first_cell = &rows[2][0];
        if let Some(s) = first_cell.get_string() {
            if s.starts_with("XHP") || s.len() > 5 {
                2 // Row 2 is data
            } else {
                3 // Row 2 is units, skip it
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
        metadata,
        animals,
    })
}
