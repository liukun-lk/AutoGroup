use crate::core::models::*;
use anyhow::{Context, Result};
use rust_xlsxwriter::{Format, Workbook};

/// Configuration for exporting grouping results
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Indicator names to include in export (in order)
    pub selected_indicators: Vec<String>,
    /// Whether to include statistics sheet
    pub include_statistics: bool,
    /// Whether to include summary sheet
    pub include_summary: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            selected_indicators: Vec::new(),
            include_statistics: true,
            include_summary: true,
        }
    }
}

/// Helper struct for organizing export rows
#[derive(Debug, Clone)]
struct ExportRow {
    group_id: usize,
    animal_id: String,
    sex: Sex,
    indicators: Vec<f64>,
}

impl ExportRow {
    fn sex_chinese(&self) -> &'static str {
        self.sex.to_chinese()
    }

    /// Sort order: group (asc) > sex (female first) > animal_id (asc)
    fn sort_key(&self) -> (usize, bool, String) {
        (
            self.group_id,
            self.sex == Sex::Male, // false for Female, true for Male
            self.animal_id.clone(),
        )
    }
}

/// Export grouping results to Excel file with multiple sheets
pub fn export_grouping_result(
    result: &GroupingResult,
    dataset: &Dataset,
    config: &ExportConfig,
    output_path: &str,
) -> Result<()> {
    let mut workbook = Workbook::new();

    // Sheet 1: Grouping results (分组结果)
    write_grouping_sheet(&mut workbook, result, dataset, config)?;

    // Sheet 2: Statistics (统计结果)
    if config.include_statistics {
        write_statistics_sheet(&mut workbook, result)?;
    }

    // Sheet 3: Summary (汇总信息)
    if config.include_summary {
        write_summary_sheet(&mut workbook, result, dataset, config)?;
    }

    workbook
        .save(output_path)
        .with_context(|| format!("Failed to save Excel file to {}", output_path))?;

    Ok(())
}

/// Write Sheet 1: Grouping results with format matching output spec
fn write_grouping_sheet(
    workbook: &mut Workbook,
    result: &GroupingResult,
    dataset: &Dataset,
    config: &ExportConfig,
) -> Result<()> {
    let sheet = workbook.add_worksheet();
    sheet
        .set_name("分组结果")
        .context("Failed to set sheet name")?;

    // Prepare export rows
    let mut export_rows = Vec::new();
    for assignment in &result.assignments {
        let animal = dataset
            .animals
            .iter()
            .find(|a| a.id == assignment.animal_id)
            .context(format!("Animal {} not found in dataset", assignment.animal_id))?;

        let indicator_values: Vec<f64> = config
            .selected_indicators
            .iter()
            .map(|name| animal.indicators.get(name).copied().unwrap_or(0.0))
            .collect();

        export_rows.push(ExportRow {
            group_id: assignment.group_id + 1, // Convert to 1-based
            animal_id: assignment.animal_id.clone(),
            sex: assignment.sex,
            indicators: indicator_values,
        });
    }

    // Sort rows: group > sex (female first) > animal_id
    export_rows.sort_by_key(|row| row.sort_key());

    // Write header row
    let header_format = Format::new().set_bold();

    sheet.write_string_with_format(0, 0, "组别", &header_format)?;
    sheet.write_string_with_format(0, 1, "动物编号", &header_format)?;
    sheet.write_string_with_format(0, 2, "性别", &header_format)?;

    for (col_idx, indicator_name) in config.selected_indicators.iter().enumerate() {
        sheet.write_string_with_format(0, (col_idx + 3) as u16, indicator_name, &header_format)?;
    }

    // Write data rows
    for (row_idx, row) in export_rows.iter().enumerate() {
        let excel_row = (row_idx + 1) as u32;

        sheet.write_number(excel_row, 0, row.group_id as f64)?;
        sheet.write_string(excel_row, 1, &row.animal_id)?;
        sheet.write_string(excel_row, 2, row.sex_chinese())?;

        for (col_idx, &value) in row.indicators.iter().enumerate() {
            sheet.write_number(excel_row, (col_idx + 3) as u16, value)?;
        }
    }

    // Auto-fit columns (approximate)
    sheet.set_column_width(0, 8)?; // Group ID
    sheet.set_column_width(1, 15)?; // Animal ID
    sheet.set_column_width(2, 8)?; // Sex

    Ok(())
}

/// Write Sheet 2: Statistical test results
fn write_statistics_sheet(workbook: &mut Workbook, result: &GroupingResult) -> Result<()> {
    let sheet = workbook.add_worksheet();
    sheet
        .set_name("统计结果")
        .context("Failed to set sheet name")?;

    // Header row
    let header_format = Format::new().set_bold();
    sheet.write_string_with_format(0, 0, "指标名称", &header_format)?;
    sheet.write_string_with_format(0, 1, "P 值", &header_format)?;
    sheet.write_string_with_format(0, 2, "检验方法", &header_format)?;
    sheet.write_string_with_format(0, 3, "是否达标", &header_format)?;

    // Data rows
    for (row_idx, stat) in result.statistics.iter().enumerate() {
        let excel_row = (row_idx + 1) as u32;

        sheet.write_string(excel_row, 0, &stat.indicator_name)?;
        sheet.write_number(excel_row, 1, stat.p_value)?;
        sheet.write_string(excel_row, 2, &stat.test_method)?;
        sheet.write_string(excel_row, 3, if stat.is_valid { "✓" } else { "✗" })?;
    }

    // Auto-fit columns
    sheet.set_column_width(0, 15)?;
    sheet.set_column_width(1, 12)?;
    sheet.set_column_width(2, 20)?;
    sheet.set_column_width(3, 10)?;

    Ok(())
}

/// Write Sheet 3: Summary information
fn write_summary_sheet(
    workbook: &mut Workbook,
    result: &GroupingResult,
    dataset: &Dataset,
    config: &ExportConfig,
) -> Result<()> {
    let sheet = workbook.add_worksheet();
    sheet
        .set_name("汇总信息")
        .context("Failed to set sheet name")?;

    let label_format = Format::new().set_bold();
    let mut row = 0u32;

    // Section 1: Dataset information
    sheet.write_string_with_format(row, 0, "数据集信息", &label_format)?;
    row += 1;

    sheet.write_string(row, 0, "总动物数")?;
    sheet.write_number(row, 1, dataset.metadata.total_animals as f64)?;
    row += 1;

    sheet.write_string(row, 0, "雄性数量")?;
    sheet.write_number(row, 1, dataset.metadata.male_count as f64)?;
    row += 1;

    sheet.write_string(row, 0, "雌性数量")?;
    sheet.write_number(row, 1, dataset.metadata.female_count as f64)?;
    row += 1;

    sheet.write_string(row, 0, "指标总数")?;
    sheet.write_number(row, 1, dataset.metadata.indicator_count as f64)?;
    row += 2;

    // Section 2: Grouping configuration
    sheet.write_string_with_format(row, 0, "分组配置", &label_format)?;
    row += 1;

    // Count groups
    let num_groups = result
        .assignments
        .iter()
        .map(|a| a.group_id)
        .max()
        .unwrap_or(0)
        + 1;
    sheet.write_string(row, 0, "分组数量")?;
    sheet.write_number(row, 1, num_groups as f64)?;
    row += 1;

    // Group composition
    for group_id in 0..num_groups {
        let group_animals: Vec<_> = result
            .assignments
            .iter()
            .filter(|a| a.group_id == group_id)
            .collect();

        let males = group_animals.iter().filter(|a| a.sex == Sex::Male).count();
        let females = group_animals
            .iter()
            .filter(|a| a.sex == Sex::Female)
            .count();

        sheet.write_string(row, 0, &format!("组 {} 配置", group_id + 1))?;
        sheet.write_string(
            row,
            1,
            &format!("{} 只 ({}雄 + {}雌)", group_animals.len(), males, females),
        )?;
        row += 1;
    }

    row += 1;

    // Section 3: Statistical configuration
    sheet.write_string_with_format(row, 0, "统计配置", &label_format)?;
    row += 1;

    sheet.write_string(row, 0, "参与统计指标数")?;
    sheet.write_number(row, 1, config.selected_indicators.len() as f64)?;
    row += 2;

    // Section 4: Results summary
    sheet.write_string_with_format(row, 0, "结果摘要", &label_format)?;
    row += 1;

    sheet.write_string(row, 0, "最小 P 值")?;
    sheet.write_number(row, 1, result.summary.min_p_value)?;
    row += 1;

    sheet.write_string(row, 0, "平均 P 值")?;
    sheet.write_number(row, 1, result.summary.mean_p_value)?;
    row += 1;

    sheet.write_string(row, 0, "不达标指标数")?;
    sheet.write_number(row, 1, result.summary.num_invalid_indicators as f64)?;
    row += 1;

    sheet.write_string(row, 0, "是否满足要求")?;
    sheet.write_string(row, 1, if result.summary.meets_criteria { "是" } else { "否" })?;
    row += 1;

    sheet.write_string(row, 0, "计算耗时 (ms)")?;
    sheet.write_number(row, 1, result.computation_time_ms as f64)?;

    // Auto-fit columns
    sheet.set_column_width(0, 20)?;
    sheet.set_column_width(1, 25)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_export_row_sorting() {
        let mut rows = vec![
            ExportRow {
                group_id: 2,
                animal_id: "M001".to_string(),
                sex: Sex::Male,
                indicators: vec![],
            },
            ExportRow {
                group_id: 1,
                animal_id: "F002".to_string(),
                sex: Sex::Female,
                indicators: vec![],
            },
            ExportRow {
                group_id: 1,
                animal_id: "M003".to_string(),
                sex: Sex::Male,
                indicators: vec![],
            },
            ExportRow {
                group_id: 1,
                animal_id: "F001".to_string(),
                sex: Sex::Female,
                indicators: vec![],
            },
        ];

        rows.sort_by_key(|r| r.sort_key());

        // Expected order:
        // Group 1: F001 (Female), F002 (Female), M003 (Male)
        // Group 2: M001 (Male)
        assert_eq!(rows[0].animal_id, "F001");
        assert_eq!(rows[1].animal_id, "F002");
        assert_eq!(rows[2].animal_id, "M003");
        assert_eq!(rows[3].animal_id, "M001");
    }

    #[test]
    fn test_export_config_default() {
        let config = ExportConfig::default();
        assert!(config.include_statistics);
        assert!(config.include_summary);
        assert_eq!(config.selected_indicators.len(), 0);
    }

    #[test]
    #[ignore] // Only run manually to test file I/O
    fn test_export_with_mock_data() {
        // Create minimal test dataset
        let mut animals = Vec::new();
        for i in 0..4 {
            let mut indicators = HashMap::new();
            indicators.insert("Weight".to_string(), 30.0 + i as f64);

            animals.push(Animal {
                id: format!("M{:03}", i + 1),
                sex: if i < 2 { Sex::Male } else { Sex::Female },
                indicators,
            });
        }

        let dataset = Dataset {
            indicator_names: vec!["Weight".to_string()],
            metadata: DatasetMetadata {
                total_animals: 4,
                male_count: 2,
                female_count: 2,
                indicator_count: 1,
            },
            animals,
        };

        let result = GroupingResult {
            assignments: vec![
                GroupAssignment {
                    animal_id: "M001".to_string(),
                    group_id: 0,
                    sex: Sex::Male,
                },
                GroupAssignment {
                    animal_id: "M003".to_string(),
                    group_id: 0,
                    sex: Sex::Female,
                },
                GroupAssignment {
                    animal_id: "M002".to_string(),
                    group_id: 1,
                    sex: Sex::Male,
                },
                GroupAssignment {
                    animal_id: "M004".to_string(),
                    group_id: 1,
                    sex: Sex::Female,
                },
            ],
            statistics: vec![IndicatorStats {
                indicator_name: "Weight".to_string(),
                p_value: 0.85,
                is_valid: true,
                test_method: "Student t-test".to_string(),
            }],
            summary: ResultSummary {
                min_p_value: 0.85,
                mean_p_value: 0.85,
                num_invalid_indicators: 0,
                meets_criteria: true,
            },
            computation_time_ms: 10,
        };

        let config = ExportConfig {
            selected_indicators: vec!["Weight".to_string()],
            include_statistics: true,
            include_summary: true,
        };

        let output_path = "/tmp/test_grouping_export.xlsx";
        export_grouping_result(&result, &dataset, &config, output_path).unwrap();

        // Verify file exists
        assert!(std::path::Path::new(output_path).exists());
        println!("Test export created at: {}", output_path);
    }
}
