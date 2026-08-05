use crate::core::models::*;
use anyhow::{Context, Result};
use rust_xlsxwriter::{Format, Workbook};

/// Configuration for exporting sheet content
#[derive(Debug, Clone)]
pub struct SheetConfig {
    /// Indicator names to include in export (in order)
    pub selected_indicators: Vec<String>,
    /// Whether to include statistics sheet
    pub include_statistics: bool,
    /// Whether to include summary sheet
    pub include_summary: bool,
    /// Group constraints (for custom naming and reserve group handling)
    pub group_constraints: Option<Vec<SexConstraint>>,
}

impl Default for SheetConfig {
    fn default() -> Self {
        Self {
            selected_indicators: Vec::new(),
            include_statistics: true,
            include_summary: true,
            group_constraints: None,
        }
    }
}

/// Helper struct for organizing export rows
#[derive(Debug, Clone)]
struct ExportRow {
    group_id: usize,
    group_name: String,
    is_reserve: bool,
    animal_id: String,
    sex: Sex,
    indicators: Vec<f64>,
}

impl ExportRow {
    fn sex_chinese(&self) -> &'static str {
        self.sex.to_chinese()
    }

    /// Sort order: reserve groups last > group_id (asc) > sex (female first) > animal_id (asc)
    fn sort_key(&self) -> (bool, usize, bool, String) {
        (
            self.is_reserve, // false for experimental, true for reserve (reserve comes last)
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
    config: &SheetConfig,
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
        .with_context(|| format!("Failed to save Excel file to {output_path}"))?;

    Ok(())
}

/// Export multiple grouping candidates to Excel with each candidate in a separate sheet
pub fn export_multiple_results(
    results: &MultiGroupingResult,
    dataset: &Dataset,
    config: &SheetConfig,
    output_path: &str,
) -> Result<()> {
    let mut workbook = Workbook::new();

    // For each candidate, create a dedicated worksheet with grouping + statistics
    for (idx, result) in results.candidates.iter().enumerate() {
        let rank = idx + 1;

        // Sheet: Candidate N - Grouping
        let grouping_sheet = workbook.add_worksheet();
        grouping_sheet
            .set_name(format!("方案{rank}-分组"))
            .with_context(|| format!("Failed to set sheet name for candidate {rank}"))?;

        write_grouping_sheet_to(grouping_sheet, result, dataset, config)?;

        // Sheet: Candidate N - Statistics
        if config.include_statistics {
            let stats_sheet = workbook.add_worksheet();
            stats_sheet
                .set_name(format!("方案{rank}-统计"))
                .with_context(|| format!("Failed to set stats sheet name for candidate {rank}"))?;

            write_statistics_sheet_to(stats_sheet, result)?;
        }
    }

    // Add summary comparison sheet
    write_comparison_sheet(&mut workbook, results)?;

    workbook
        .save(output_path)
        .with_context(|| format!("Failed to save Excel file to {output_path}"))?;

    Ok(())
}

/// Write Sheet 1: Grouping results with dual-row header matching 动物分组 format
fn write_grouping_sheet(
    workbook: &mut Workbook,
    result: &GroupingResult,
    dataset: &Dataset,
    config: &SheetConfig,
) -> Result<()> {
    let sheet = workbook.add_worksheet();
    sheet
        .set_name("分组结果")
        .context("Failed to set sheet name")?;

    write_grouping_sheet_to(sheet, result, dataset, config)
}

/// Write grouping data to a specific worksheet
fn write_grouping_sheet_to(
    sheet: &mut rust_xlsxwriter::Worksheet,
    result: &GroupingResult,
    dataset: &Dataset,
    config: &SheetConfig,
) -> Result<()> {
    // Build group constraint lookup map
    let group_constraints_map: std::collections::HashMap<usize, &SexConstraint> = config
        .group_constraints
        .as_ref()
        .map(|constraints| constraints.iter().map(|c| (c.group_index, c)).collect())
        .unwrap_or_default();

    // Prepare export rows
    let mut export_rows = Vec::new();
    for assignment in &result.assignments {
        let animal = dataset
            .animals
            .iter()
            .find(|a| a.id == assignment.animal_id)
            .context(format!(
                "Animal {} not found in dataset",
                assignment.animal_id
            ))?;

        let indicator_values: Vec<f64> = config
            .selected_indicators
            .iter()
            .map(|name| animal.indicators.get(name).copied().unwrap_or(0.0))
            .collect();

        // Get group metadata
        let constraint = group_constraints_map.get(&assignment.group_id);
        let is_reserve = constraint
            .map(|c| c.group_type == GroupType::Reserve)
            .unwrap_or(false);

        let group_name = constraint
            .and_then(|c| c.custom_name.clone())
            .unwrap_or_else(|| format!("组{}", assignment.group_id + 1));

        export_rows.push(ExportRow {
            group_id: assignment.group_id,
            group_name,
            is_reserve,
            animal_id: assignment.animal_id.clone(),
            sex: assignment.sex,
            indicators: indicator_values,
        });
    }

    // Sort rows: experimental groups first (by group_id), then reserve groups last
    export_rows.sort_by_key(|row| row.sort_key());

    let header_format = Format::new().set_bold();

    // Row 0: Unit row (empty for first 3 columns, units from column 4+)
    // Columns 1-3 are empty in Row 0
    for (col_idx, indicator_key) in config.selected_indicators.iter().enumerate() {
        if let Some(metadata) = dataset.get_indicator_metadata(indicator_key) {
            if !metadata.unit.is_empty() {
                sheet.write_string(0, (col_idx + 3) as u16, &metadata.unit)?;
            }
        }
    }

    // Row 1: Column name row
    sheet.write_string_with_format(1, 0, "组别", &header_format)?;
    sheet.write_string_with_format(1, 1, "动物编号", &header_format)?;
    sheet.write_string_with_format(1, 2, "性别", &header_format)?;

    for (col_idx, indicator_key) in config.selected_indicators.iter().enumerate() {
        let display_name = if let Some(metadata) = dataset.get_indicator_metadata(indicator_key) {
            &metadata.display_name
        } else {
            indicator_key
        };
        sheet.write_string_with_format(1, (col_idx + 3) as u16, display_name, &header_format)?;
    }

    // Group rows by group_id for statistics calculation
    use std::collections::BTreeMap;
    let mut groups_data: BTreeMap<(usize, bool), Vec<&ExportRow>> = BTreeMap::new();
    for row in &export_rows {
        groups_data
            .entry((row.group_id, row.is_reserve))
            .or_default()
            .push(row);
    }

    // Write data rows and statistics rows
    let mut current_excel_row = 2u32;

    for ((group_id, is_reserve), group_rows) in groups_data.iter() {
        // Write animal data rows for this group
        for row in group_rows {
            sheet.write_string(current_excel_row, 0, &row.group_name)?;
            sheet.write_string(current_excel_row, 1, &row.animal_id)?;
            sheet.write_string(current_excel_row, 2, row.sex_chinese())?;

            for (col_idx, &value) in row.indicators.iter().enumerate() {
                sheet.write_number(current_excel_row, (col_idx + 3) as u16, value)?;
            }
            current_excel_row += 1;
        }

        // Add statistics row only for experimental groups (not reserve groups)
        if !is_reserve {
            let num_indicators = config.selected_indicators.len();

            // Write row label and group number
            sheet.write_string(current_excel_row, 0, "均值±标准差")?;
            sheet.write_number(current_excel_row, 1, (group_id + 1) as f64)?;
            sheet.write_string(current_excel_row, 2, "")?; // Empty sex column

            // Calculate and write mean±std for each indicator
            for indicator_idx in 0..num_indicators {
                let values: Vec<f64> = group_rows
                    .iter()
                    .map(|r| r.indicators[indicator_idx])
                    .collect();

                let (mean, std) = calculate_mean_std(&values);
                let formatted = format!("{:.2}±{:.2}", mean, std);
                sheet.write_string(current_excel_row, (indicator_idx + 3) as u16, &formatted)?;
            }
            current_excel_row += 1;
        }

        // Add a blank row after each group for better readability
        current_excel_row += 1;
    }

    // Auto-fit columns (approximate)
    sheet.set_column_width(0, 8)?; // Group ID
    sheet.set_column_width(1, 15)?; // Animal ID
    sheet.set_column_width(2, 8)?; // Sex

    Ok(())
}

/// Calculate mean and standard deviation for a set of values
fn calculate_mean_std(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    if n == 0.0 {
        return (0.0, 0.0);
    }

    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std = variance.sqrt();

    (mean, std)
}

/// Write Sheet 2: Statistical test results
fn write_statistics_sheet(workbook: &mut Workbook, result: &GroupingResult) -> Result<()> {
    let sheet = workbook.add_worksheet();
    sheet
        .set_name("统计结果")
        .context("Failed to set sheet name")?;

    write_statistics_sheet_to(sheet, result)
}

/// Write statistics data to a specific worksheet
fn write_statistics_sheet_to(
    sheet: &mut rust_xlsxwriter::Worksheet,
    result: &GroupingResult,
) -> Result<()> {
    // Header row
    let header_format = Format::new().set_bold();
    sheet.write_string_with_format(0, 0, "指标名称", &header_format)?;
    sheet.write_string_with_format(0, 1, "Levene P 值", &header_format)?;
    sheet.write_string_with_format(0, 2, "差异检验 P 值", &header_format)?;
    sheet.write_string_with_format(0, 3, "检验方法", &header_format)?;
    sheet.write_string_with_format(0, 4, "是否达标", &header_format)?;

    // Data rows
    for (row_idx, stat) in result.statistics.iter().enumerate() {
        let excel_row = (row_idx + 1) as u32;

        sheet.write_string(excel_row, 0, &stat.indicator_name)?;
        sheet.write_number(excel_row, 1, stat.levene_p_value)?;
        sheet.write_number(excel_row, 2, stat.diff_p_value)?;
        sheet.write_string(excel_row, 3, &stat.test_method)?;
        sheet.write_string(excel_row, 4, if stat.is_valid { "✓" } else { "✗" })?;
    }

    // Auto-fit columns
    sheet.set_column_width(0, 15)?;
    sheet.set_column_width(1, 12)?;
    sheet.set_column_width(2, 14)?;
    sheet.set_column_width(3, 20)?;
    sheet.set_column_width(4, 10)?;

    Ok(())
}

/// Write Sheet 3: Summary information
fn write_summary_sheet(
    workbook: &mut Workbook,
    result: &GroupingResult,
    dataset: &Dataset,
    config: &SheetConfig,
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

        sheet.write_string(row, 0, format!("组 {} 配置", group_id + 1))?;
        sheet.write_string(
            row,
            1,
            format!("{} 只 ({}雄 + {}雌)", group_animals.len(), males, females),
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
    sheet.write_string(
        row,
        1,
        if result.summary.meets_criteria {
            "是"
        } else {
            "否"
        },
    )?;
    row += 1;

    sheet.write_string(row, 0, "计算耗时 (ms)")?;
    sheet.write_number(row, 1, result.computation_time_ms as f64)?;

    // Auto-fit columns
    sheet.set_column_width(0, 20)?;
    sheet.set_column_width(1, 25)?;

    Ok(())
}

/// Write comparison sheet for multiple candidates
fn write_comparison_sheet(workbook: &mut Workbook, results: &MultiGroupingResult) -> Result<()> {
    let sheet = workbook.add_worksheet();
    sheet
        .set_name("方案对比")
        .context("Failed to set comparison sheet name")?;

    let header_format = Format::new().set_bold();
    let label_format = Format::new().set_bold();

    // Header row
    sheet.write_string_with_format(0, 0, "排名", &header_format)?;
    sheet.write_string_with_format(0, 1, "最小 P 值", &header_format)?;
    sheet.write_string_with_format(0, 2, "平均 P 值", &header_format)?;
    sheet.write_string_with_format(0, 3, "不达标指标数", &header_format)?;
    sheet.write_string_with_format(0, 4, "达标指标数", &header_format)?;
    sheet.write_string_with_format(0, 5, "总指标数", &header_format)?;
    sheet.write_string_with_format(0, 6, "是否满足要求", &header_format)?;

    // Data rows
    for (idx, result) in results.candidates.iter().enumerate() {
        let excel_row = (idx + 1) as u32;
        let rank = idx + 1;

        sheet.write_number(excel_row, 0, rank as f64)?;
        sheet.write_number(excel_row, 1, result.summary.min_p_value)?;
        sheet.write_number(excel_row, 2, result.summary.mean_p_value)?;
        sheet.write_number(excel_row, 3, result.summary.num_invalid_indicators as f64)?;
        sheet.write_number(excel_row, 4, result.summary.passed_indicators as f64)?;
        sheet.write_number(excel_row, 5, result.summary.total_indicators as f64)?;
        sheet.write_string(
            excel_row,
            6,
            if result.summary.meets_criteria {
                "是"
            } else {
                "否"
            },
        )?;
    }

    let mut row = (results.candidates.len() + 2) as u32;

    // Summary statistics
    sheet.write_string_with_format(row, 0, "评估统计", &label_format)?;
    row += 1;

    sheet.write_string(row, 0, "总评估候选数")?;
    sheet.write_number(row, 1, results.total_evaluated as f64)?;
    row += 1;

    sheet.write_string(row, 0, "合格候选数")?;
    sheet.write_number(row, 1, results.total_valid as f64)?;
    row += 1;

    sheet.write_string(row, 0, "计算耗时 (ms)")?;
    sheet.write_number(row, 1, results.computation_time_ms as f64)?;

    // Auto-fit columns
    sheet.set_column_width(0, 8)?;
    sheet.set_column_width(1, 12)?;
    sheet.set_column_width(2, 12)?;
    sheet.set_column_width(3, 14)?;
    sheet.set_column_width(4, 14)?;
    sheet.set_column_width(5, 12)?;
    sheet.set_column_width(6, 14)?;

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
                group_name: "组3".to_string(),
                is_reserve: false,
                animal_id: "M001".to_string(),
                sex: Sex::Male,
                indicators: vec![],
            },
            ExportRow {
                group_id: 1,
                group_name: "组2".to_string(),
                is_reserve: false,
                animal_id: "F002".to_string(),
                sex: Sex::Female,
                indicators: vec![],
            },
            ExportRow {
                group_id: 1,
                group_name: "组2".to_string(),
                is_reserve: false,
                animal_id: "M003".to_string(),
                sex: Sex::Male,
                indicators: vec![],
            },
            ExportRow {
                group_id: 1,
                group_name: "组2".to_string(),
                is_reserve: false,
                animal_id: "F001".to_string(),
                sex: Sex::Female,
                indicators: vec![],
            },
            ExportRow {
                group_id: 3,
                group_name: "备用动物".to_string(),
                is_reserve: true,
                animal_id: "R001".to_string(),
                sex: Sex::Male,
                indicators: vec![],
            },
        ];

        rows.sort_by_key(|r| r.sort_key());

        // Expected order:
        // Experimental groups first (by group_id):
        //   Group 1: F001 (Female), F002 (Female), M003 (Male)
        //   Group 2: M001 (Male)
        // Reserve groups last:
        //   Reserve: R001 (Male)
        assert_eq!(rows[0].animal_id, "F001");
        assert_eq!(rows[1].animal_id, "F002");
        assert_eq!(rows[2].animal_id, "M003");
        assert_eq!(rows[3].animal_id, "M001");
        assert_eq!(rows[4].animal_id, "R001");
        assert_eq!(rows[4].group_name, "备用动物");
    }

    #[test]
    fn test_export_config_default() {
        let config = SheetConfig::default();
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
            indicator_metadata: vec![IndicatorMetadata::new(
                "Weight".to_string(),
                "Weight".to_string(),
                "kg".to_string(),
            )],
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
                levene_p_value: 0.92,
                diff_p_value: 0.85,
                is_valid: true,
                test_method: "Student t-test".to_string(),
                posthoc_results: None,
            }],
            summary: ResultSummary {
                min_p_value: 0.85,
                mean_p_value: 0.85,
                num_invalid_indicators: 0,
                meets_criteria: true,
                total_animals: 4,
                num_groups: 2,
                passed_indicators: 1,
                total_indicators: 1,
            },
            computation_time_ms: 10,
        };

        let config = SheetConfig {
            selected_indicators: vec!["Weight".to_string()],
            include_statistics: true,
            include_summary: true,
            group_constraints: None,
        };

        let output_path = "/tmp/test_grouping_export.xlsx";
        export_grouping_result(&result, &dataset, &config, output_path).unwrap();

        // Verify file exists
        assert!(std::path::Path::new(output_path).exists());
        println!("Test export created at: {output_path}");
    }
}
